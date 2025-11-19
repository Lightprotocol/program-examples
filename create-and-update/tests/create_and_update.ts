import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { CreateAndUpdate } from "../target/types/create_and_update";
import {
  bn,
  CompressedAccountWithMerkleContext,
  confirmTx,
  createRpc,
  defaultTestStateTreeAccounts,
  deriveAddress,
  deriveAddressSeed,
  featureFlags,
  PackedAccounts,
  Rpc,
  sleep,
  SystemAccountMetaConfig,
  getLightSystemAccountMetasV2,
  VERSION,
  selectStateTreeInfo,
  getDefaultAddressTreeInfo,
  AddressTreeInfo,
  TreeInfo,
  packTreeInfos,
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
// set to V2 explicitly.
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
    const addressTreeInfo = getDefaultAddressTreeInfo();
    console.log("addressTreeInfo", addressTreeInfo);

    const firstSeed = new TextEncoder().encode("first");

    // v2 derive
    const firstAddressSeed = deriveAddressSeedV2([
      firstSeed,
      signer.publicKey.toBytes(),
    ]);
    const firstAddress = deriveAddressV2(
      firstAddressSeed,
      addressTreeInfo.tree,
      program.programId
    );
    console.log("firstAddressSeed", Array.from(firstAddressSeed));
    console.log("firstAddress", Array.from(firstAddress.toBytes()));

    await createCompressedAccount(
      rpc,
      addressTreeInfo,
      firstAddress,
      program,
      stateTreeInfo,
      signer,
      "Initial message"
    );

    // await waitForIndexer(rpc);

    let firstAccount = await rpc.getCompressedAccount(
      bn(firstAddress.toBytes())
    );
    if (!firstAccount) {
      throw new Error("Failed to fetch the initial compressed account");
    }

    let decoded = coder.types.decode("DataAccount", firstAccount.data.data);
    assert.ok(
      decoded.owner.equals(signer.publicKey),
      "owner should match signer"
    );
    assert.strictEqual(decoded.message, "Initial message");

    //     await createAndUpdateAccounts(
    //       rpc,
    //       program,
    //       signer,
    //       firstAccount,
    //       secondAddress,
    //       addressTree,
    //       addressQueue,
    //       outputStateTree,
    //       "Initial message",
    //       "Hello from second account",
    //       "Updated first message"
    //     );

    //     await waitForIndexer(rpc);

    //     firstAccount = await rpc.getCompressedAccount(bn(initialAddress.toBytes()));
    //     if (!firstAccount) {
    //       throw new Error("Initial account missing after create_and_update");
    //     }
    //     decoded = coder.types.decode("DataAccount", firstAccount.data.data);
    //     assert.strictEqual(decoded.message, "Updated first message");

    //     let secondAccount = await rpc.getCompressedAccount(
    //       bn(secondAddress.toBytes())
    //     );
    //     if (!secondAccount) {
    //       throw new Error("Failed to fetch the second compressed account");
    //     }
    //     let secondDecoded = coder.types.decode(
    //       "DataAccount",
    //       secondAccount.data.data
    //     );
    //     assert.strictEqual(secondDecoded.message, "Hello from second account");
    //     assert.ok(
    //       secondDecoded.owner.equals(signer.publicKey),
    //       "new account owner should match signer"
    //     );

    //     await updateTwoExistingAccounts(
    //       rpc,
    //       program,
    //       signer,
    //       firstAccount,
    //       secondAccount,
    //       outputStateTree,
    //       "Updated first message",
    //       "First account final message",
    //       "Hello from second account",
    //       "Second account final message"
    //     );

    //     await waitForIndexer(rpc);

    //     firstAccount = await rpc.getCompressedAccount(bn(initialAddress.toBytes()));
    //     secondAccount = await rpc.getCompressedAccount(bn(secondAddress.toBytes()));
    //     if (!firstAccount || !secondAccount) {
    //       throw new Error(
    //         "One of the accounts is missing after update_two_accounts"
    //       );
    //     }

    //     decoded = coder.types.decode("DataAccount", firstAccount.data.data);
    //     secondDecoded = coder.types.decode("DataAccount", secondAccount.data.data);

    //     assert.strictEqual(decoded.message, "First account final message");
    //     assert.strictEqual(secondDecoded.message, "Second account final message");
    //     assert.ok(
    //       decoded.owner.equals(signer.publicKey) &&
    //         secondDecoded.owner.equals(signer.publicKey),
    //       "owners should remain the signer"
    //     );
    //   });
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
    addressTree: anchor.web3.PublicKey,
    addressQueue: anchor.web3.PublicKey,
    outputStateTree: anchor.web3.PublicKey,
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
      [
        {
          tree: addressTree,
          queue: addressQueue,
          address: bn(newAddress.toBytes()),
        },
      ]
    );
    const config = SystemAccountMetaConfig.new(program.programId);
    const packedAccounts = PackedAccounts.newWithSystemAccountsV2(config);
    const existingMerkleTreeIndex = packedAccounts.insertOrGet(
      existingAccount.treeInfo.tree
    );
    const existingQueueIndex = packedAccounts.insertOrGet(
      existingAccount.treeInfo.queue
    );
    const outputStateTreeIndex = packedAccounts.insertOrGet(outputStateTree);

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

    const packedAddressTreeInfo = {
      rootIndex: proofRpcResult.rootIndices[1],
      addressMerkleTreePubkeyIndex: packedAccounts.insertOrGet(addressTree),
      addressQueuePubkeyIndex: packedAccounts.insertOrGet(addressQueue),
    };

    const proof = {
      0: proofRpcResult.compressedProof,
    };
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });

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
      .remainingAccounts(packedAccounts.toAccountMetas().remainingAccounts)
      .signers([signer])
      .transaction();

    tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
    tx.sign(signer);

    const sig = await rpc.sendTransaction(tx, [signer]);
    await confirmTx(rpc, sig);
    return sig;
  }
});
