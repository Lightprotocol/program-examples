import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { Burn } from "../target/types/burn";
import burnIdl from "../target/idl/burn.json";
import {
  bn,
  CompressedAccountWithMerkleContext,
  confirmTx,
  createRpc,
  defaultTestStateTreeAccounts,
  deriveAddressV2,
  deriveAddressSeedV2,
  batchAddressTree,
  PackedAccounts,
  Rpc,
  sleep,
  SystemAccountMetaConfig,
  featureFlags,
  VERSION,
} from "@lightprotocol/stateless.js";
import * as assert from "assert";

// Force V2 mode
(featureFlags as any).version = VERSION.V2;

const path = require("path");
const os = require("os");
require("dotenv").config();

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-anchor-burn", () => {
  const burnProgram = anchor.workspace.Burn as Program<Burn>;
  const burnCoder = new anchor.BorshCoder(burnIdl as anchor.Idl);

  it("burn compressed account", async () => {
    let signer = new web3.Keypair();
    let rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      {
        commitment: "confirmed",
      },
    );
    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(signer.publicKey, lamports);
    await sleep(2000);

    const outputStateTree = defaultTestStateTreeAccounts().merkleTree;
    const addressTree = new web3.PublicKey(batchAddressTree);

    const messageSeed = new TextEncoder().encode("message");
    const seed = deriveAddressSeedV2([messageSeed, signer.publicKey.toBytes()]);
    const address = deriveAddressV2(
      seed,
      addressTree,
      new web3.PublicKey(burnProgram.idl.address),
    );

    // Step 1: Create compressed account with initial message
    const createTxId = await createCompressedAccount(
      rpc,
      addressTree,
      address,
      burnProgram,
      outputStateTree,
      signer,
      "Hello, compressed world!",
    );
    console.log("Create Transaction ID:", createTxId);

    // Wait for indexer to process the create transaction
    let slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    // Step 2: Get the created account and verify
    let compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    let myAccount = burnCoder.types.decode(
      "MyCompressedAccount",
      compressedAccount.data.data,
    );
    assert.strictEqual(myAccount.message, "Hello, compressed world!");
    assert.ok(myAccount.owner.equals(signer.publicKey), "Owner should match signer public key");
    console.log("Created message:", myAccount.message);

    // Step 3: Burn the account permanently
    const burnTxId = await burnCompressedAccount(
      rpc,
      compressedAccount,
      burnProgram,
      signer,
      "Hello, compressed world!",
    );
    console.log("Burn Transaction ID:", burnTxId);

    // Wait for indexer to process the burn transaction
    slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    // Step 4: Verify the account is burned (does not exist)
    try {
      await rpc.getCompressedAccount(bn(address.toBytes()));
      assert.fail("Expected account to not exist after burning");
    } catch (error: any) {
      // Account should not exist after burn
      console.log("Verified account was burned");
    }
  });
});

async function createCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Burn>,
  outputStateTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
  message: string,
) {
  const proofRpcResult = await rpc.getValidityProofV0(
    [],
    [
      {
        tree: addressTree,
        queue: addressTree,
        address: bn(address.toBytes()),
      },
    ],
  );
  const systemAccountConfig = new SystemAccountMetaConfig(program.programId);
  let remainingAccounts = new PackedAccounts();
  remainingAccounts.addSystemAccountsV2(systemAccountConfig);

  const addressMerkleTreePubkeyIndex =
    remainingAccounts.insertOrGet(addressTree);
  const addressQueuePubkeyIndex = addressMerkleTreePubkeyIndex;
  const packedAddressTreeInfo = {
    rootIndex: proofRpcResult.rootIndices[0],
    addressMerkleTreePubkeyIndex,
    addressQueuePubkeyIndex,
  };
  const outputStateTreeIndex =
    remainingAccounts.insertOrGet(outputStateTree);

  let proof = {
    0: proofRpcResult.compressedProof,
  };
  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });
  let tx = await program.methods
    .createAccount(proof, packedAddressTreeInfo, outputStateTreeIndex, message)
    .accounts({
      signer: signer.publicKey,
    })
    .preInstructions([computeBudgetIx])
    .remainingAccounts(remainingAccounts.toAccountMetas().remainingAccounts)
    .signers([signer])
    .transaction();
  tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
  tx.sign(signer);

  const sig = await rpc.sendTransaction(tx, [signer]);
  await confirmTx(rpc, sig);
  return sig;
}

async function burnCompressedAccount(
  rpc: Rpc,
  compressedAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Burn>,
  signer: anchor.web3.Keypair,
  currentMessage: string,
) {
  const proofRpcResult = await rpc.getValidityProofV0(
    [
      {
        hash: compressedAccount.hash,
        tree: compressedAccount.treeInfo.tree,
        queue: compressedAccount.treeInfo.queue,
      },
    ],
    [],
  );

  const systemAccountConfig = new SystemAccountMetaConfig(program.programId);
  let remainingAccounts = new PackedAccounts();
  remainingAccounts.addSystemAccountsV2(systemAccountConfig);

  const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.tree,
  );
  const queuePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.queue,
  );

  // CompressedAccountMetaBurn does not have output_state_tree_index
  const compressedAccountMeta = {
    treeInfo: {
      merkleTreePubkeyIndex,
      queuePubkeyIndex,
      leafIndex: compressedAccount.leafIndex,
      proveByIndex: false,
      rootIndex: proofRpcResult.rootIndices[0],
    },
    address: compressedAccount.address,
  };

  let proof = {
    0: proofRpcResult.compressedProof,
  };
  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });
  let tx = await program.methods
    .burnAccount(proof, compressedAccountMeta, currentMessage)
    .accounts({
      signer: signer.publicKey,
    })
    .preInstructions([computeBudgetIx])
    .remainingAccounts(remainingAccounts.toAccountMetas().remainingAccounts)
    .signers([signer])
    .transaction();
  tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
  tx.sign(signer);

  const sig = await rpc.sendTransaction(tx, [signer]);
  await confirmTx(rpc, sig);
  return sig;
}
