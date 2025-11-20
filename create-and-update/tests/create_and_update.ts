import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { CreateAndUpdate } from "../target/types/create_and_update";
import {
  bn,
  CompressedAccountWithMerkleContext,
  confirmTx,
  createRpc,
  featureFlags,
  PackedAccounts,
  Rpc,
  sleep,
  SystemAccountMetaConfig,
  VERSION,
  selectStateTreeInfo,
  TreeInfo,
  PackedAddressTreeInfo,
  deriveAddressV2,
  deriveAddressSeedV2,
  buildAndSignTx,
  sendAndConfirmTx,
} from "@lightprotocol/stateless.js";
import * as assert from "assert";
const path = require("path");
const os = require("os");
require("dotenv").config();

// v2 feature flag
featureFlags.version = VERSION.V2;

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("create-and-update anchor", () => {
  const program = anchor.workspace.CreateAndUpdate as Program<CreateAndUpdate>;
  const coder = new anchor.BorshCoder(program.idl);

  it("creates and updates compressed accounts atomically", async () => {
    const signer = new web3.Keypair();
    const rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      {
        commitment: "confirmed",
      }
    );

    await rpc.requestAirdrop(signer.publicKey, web3.LAMPORTS_PER_SOL);
    await sleep(2000);

    // get tree infos
    const stateTreeInfos = await rpc.getStateTreeInfos();
    const stateTreeInfo = selectStateTreeInfo(stateTreeInfos);

    // v2 address tree info
    const addressTreeInfo = await rpc.getAddressTreeInfoV2();
    // v2 derive
    const firstSeed = new TextEncoder().encode("first");
    const firstAddressSeed = deriveAddressSeedV2([
      firstSeed,
      signer.publicKey.toBytes(),
    ]);
    const firstAddress = deriveAddressV2(
      firstAddressSeed,
      addressTreeInfo.tree,
      program.programId
    );

    await createCompressedAccount(
      rpc,
      addressTreeInfo,
      firstAddress,
      program,
      stateTreeInfo,
      signer,
      "Initial message"
    );

    let firstAccount = await rpc.getCompressedAccount(
      bn(firstAddress.toBytes())
    );
    if (!firstAccount) {
      throw new Error("Failed to fetch the initial compressed account");
    }

    let decoded = coder.types.decode("dataAccount", firstAccount.data.data);
    assert.ok(
      decoded.owner.equals(signer.publicKey),
      "owner should match signer"
    );
    assert.strictEqual(decoded.message, "Initial message");

    // Derive second address
    const secondSeed = new TextEncoder().encode("second");
    const secondAddressSeed = deriveAddressSeedV2([
      secondSeed,
      signer.publicKey.toBytes(),
    ]);
    const secondAddress = deriveAddressV2(
      secondAddressSeed,
      addressTreeInfo.tree,
      program.programId
    );
    console.log("secondAddress", Array.from(secondAddress.toBytes()));
    console.log("secondAddressSeed", Array.from(secondAddressSeed));

    await createAndUpdateAccounts(
      rpc,
      program,
      signer,
      firstAccount,
      secondAddress,
      addressTreeInfo,
      "Initial message",
      "Hello from second account",
      "Updated first message"
    );
  });

  async function waitForIndexer(rpc: Rpc) {
    const slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);
  }

  async function createCompressedAccount(
    rpc: Rpc,
    addressTreeInfo: TreeInfo,
    address: anchor.web3.PublicKey,
    program: Program<CreateAndUpdate>,
    stateTreeInfo: TreeInfo,
    signer: anchor.web3.Keypair,
    message: string
  ) {
    const proofRpcResult = await rpc.getValidityProofV0(
      [],
      [
        {
          tree: addressTreeInfo.tree,
          queue: addressTreeInfo.queue,
          address: bn(address.toBytes()),
        },
      ]
    );

    const config = SystemAccountMetaConfig.new(program.programId);
    const packedAccounts = PackedAccounts.newWithSystemAccountsV2(config);

    const outputStateTreeIndex = packedAccounts.insertOrGet(
      stateTreeInfo.queue
    );
    const addressQueueIndex = packedAccounts.insertOrGet(addressTreeInfo.queue);
    const addressTreeIndex = packedAccounts.insertOrGet(addressTreeInfo.tree);
    const packedAddressTreeInfo: PackedAddressTreeInfo = {
      rootIndex: proofRpcResult.rootIndices[0],
      addressMerkleTreePubkeyIndex: addressTreeIndex,
      addressQueuePubkeyIndex: addressQueueIndex,
    };
    const proof = {
      0: proofRpcResult.compressedProof,
    };
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 300_000,
    });

    const remainingAccounts = packedAccounts.toAccountMetas().remainingAccounts;
    for (const account of remainingAccounts) {
      console.log("remainingAccount", account.pubkey.toBase58());
    }
    const tx = await program.methods
      .createCompressedAccount(
        proof,
        packedAddressTreeInfo,
        outputStateTreeIndex,
        message
      )
      .accounts({
        signer: signer.publicKey,
      })
      .preInstructions([computeBudgetIx])
      .remainingAccounts(remainingAccounts)
      .signers([signer])
      .transaction();

    const recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;

    const signedTx = buildAndSignTx(tx.instructions, signer, recentBlockhash);
    const sig = await sendAndConfirmTx(rpc, signedTx, { skipPreflight: true });
    console.log("createCompressedAccount sig", sig);
    return sig;
  }

  async function createAndUpdateAccounts(
    rpc: Rpc,
    program: Program<CreateAndUpdate>,
    signer: anchor.web3.Keypair,
    existingAccount: CompressedAccountWithMerkleContext,
    newAddress: anchor.web3.PublicKey,
    addressTreeInfo: TreeInfo,
    existingMessage: string,
    newAccountMessage: string,
    updatedMessage: string
  ) {
    if (!existingAccount.address) {
      throw new Error("Existing compressed account missing address data");
    }

    const proofRpcResult = await rpc.getValidityProofV0(
      [
        {
          hash: existingAccount.hash,
          tree: existingAccount.treeInfo.tree,
          queue: existingAccount.treeInfo.queue,
        },
      ],
      // new account's address
      [
        {
          tree: addressTreeInfo.tree,
          queue: addressTreeInfo.queue,
          address: bn(newAddress.toBytes()),
        },
      ]
    );

    console.log("existing hash", existingAccount.hash.toArray());

    const config = SystemAccountMetaConfig.new(program.programId);
    const packedAccounts = PackedAccounts.newWithSystemAccountsV2(config);

    const existingQueueIndex = packedAccounts.insertOrGet(
      existingAccount.treeInfo.queue
    );
    const existingMerkleTreeIndex = packedAccounts.insertOrGet(
      existingAccount.treeInfo.tree
    );
    const outputStateTreeIndex = packedAccounts.insertOrGet(
      existingAccount.treeInfo.queue
    );

    const existingAccountMeta = {
      treeInfo: {
        merkleTreePubkeyIndex: existingMerkleTreeIndex,
        queuePubkeyIndex: existingQueueIndex,
        leafIndex: existingAccount.leafIndex,
        proveByIndex: false,
        rootIndex: proofRpcResult.rootIndices[0],
      },
      address: existingAccount.address,
      outputStateTreeIndex,
    };

    const addressQueueIndex = packedAccounts.insertOrGet(addressTreeInfo.queue);
    const addressTreeIndex = packedAccounts.insertOrGet(addressTreeInfo.tree);
    const packedAddressTreeInfo: PackedAddressTreeInfo = {
      rootIndex: proofRpcResult.rootIndices[1],
      addressMerkleTreePubkeyIndex: addressTreeIndex,
      addressQueuePubkeyIndex: addressQueueIndex,
    };

    const proof = {
      0: proofRpcResult.compressedProof,
    };
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });

    const remainingAccounts = packedAccounts.toAccountMetas().remainingAccounts;
    for (const account of remainingAccounts) {
      console.log("remainingAccount", account.pubkey.toBase58());
    }
    const tx = await program.methods
      .createAndUpdate(
        proof,
        {
          accountMeta: existingAccountMeta,
          message: existingMessage,
          updateMessage: updatedMessage,
        },
        {
          addressTreeInfo: packedAddressTreeInfo,
          message: newAccountMessage,
        }
      )
      .accounts({
        signer: signer.publicKey,
      })
      .preInstructions([computeBudgetIx])
      .remainingAccounts(remainingAccounts)
      .signers([signer])
      .transaction();

    const recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
    const signedTx = buildAndSignTx(tx.instructions, signer, recentBlockhash);
    const sig = await sendAndConfirmTx(rpc, signedTx, { skipPreflight: false });
    console.log("createAndUpdate sig", sig);
    return sig;
  }
});
