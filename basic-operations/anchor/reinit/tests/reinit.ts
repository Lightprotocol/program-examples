import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { Reinit } from "../target/types/reinit";
import reinitIdl from "../target/idl/reinit.json";
import {
  bn,
  CompressedAccountWithMerkleContext,
  confirmTx,
  createRpc,
  deriveAddressV2,
  deriveAddressSeedV2,
  batchAddressTree,
  PackedAccounts,
  Rpc,
  sleep,
  SystemAccountMetaConfig,
  featureFlags,
  VERSION,
  selectStateTreeInfo,
  TreeInfo,
} from "@lightprotocol/stateless.js";
import * as assert from "assert";

// Force V2 mode
(featureFlags as any).version = VERSION.V2;

const path = require("path");
const os = require("os");
require("dotenv").config();

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-anchor-reinit", () => {
  const reinitProgram = anchor.workspace.Reinit as Program<Reinit>;
  const reinitCoder = new anchor.BorshCoder(reinitIdl as anchor.Idl);

  it("reinitialize compressed account", async () => {
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

    const stateTreeInfos = await rpc.getStateTreeInfos();
    const stateTreeInfo = selectStateTreeInfo(stateTreeInfos);
    const addressTree = new web3.PublicKey(batchAddressTree);

    const messageSeed = new TextEncoder().encode("message");
    const seed = deriveAddressSeedV2([messageSeed, signer.publicKey.toBytes()]);
    const address = deriveAddressV2(
      seed,
      addressTree,
      new web3.PublicKey(reinitProgram.idl.address),
    );

    const createTxId = await createCompressedAccount(
      rpc,
      addressTree,
      address,
      reinitProgram,
      stateTreeInfo,
      signer,
      "Hello, compressed world!",
    );
    console.log("Create Transaction ID:", createTxId);

    // Wait for indexer to process the transaction
    let slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    let compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    let myAccount = reinitCoder.types.decode(
      "MyCompressedAccount",
      compressedAccount.data.data,
    );
    assert.strictEqual(myAccount.message, "Hello, compressed world!");
    assert.ok(myAccount.owner.equals(signer.publicKey), "Owner should match signer public key");
    console.log("Created message:", myAccount.message);

    const closeTxId = await closeCompressedAccount(
      rpc,
      compressedAccount,
      reinitProgram,
      stateTreeInfo,
      signer,
      "Hello, compressed world!",
    );
    console.log("Close Transaction ID:", closeTxId);

    // Wait for indexer to process the close transaction
    slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    let closedCompressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));

    // The getValidityProofV0 call will fetch the current closed account state.
    const reinitTxId = await reinitCompressedAccount(
      rpc,
      closedCompressedAccount,
      reinitProgram,
      stateTreeInfo,
      signer,
    );
    console.log("Reinit Transaction ID:", reinitTxId);

    // Wait for indexer to process the reinit transaction
    slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    // Verify the account was reinitialized with default values
    let reinitializedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    let reinitMyAccount = reinitCoder.types.decode(
      "MyCompressedAccount",
      reinitializedAccount.data.data,
    );
    assert.strictEqual(reinitMyAccount.message, "", "Message should be empty (default)");
    assert.ok(
      reinitMyAccount.owner.equals(web3.PublicKey.default),
      "Owner should be default PublicKey"
    );
    console.log("Compressed account was reinitialized with default values");
  });
});

async function createCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Reinit>,
  stateTreeInfo: TreeInfo,
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
    remainingAccounts.insertOrGet(stateTreeInfo.queue);

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

async function closeCompressedAccount(
  rpc: Rpc,
  compressedAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Reinit>,
  stateTreeInfo: TreeInfo,
  signer: anchor.web3.Keypair,
  message: string,
) {
  const systemAccountConfig = new SystemAccountMetaConfig(program.programId);
  let remainingAccounts = new PackedAccounts();
  remainingAccounts.addSystemAccountsV2(systemAccountConfig);

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

  const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.tree,
  );
  const queuePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.queue,
  );
  const outputStateTreeIndex =
    remainingAccounts.insertOrGet(stateTreeInfo.queue);

  const coder = new anchor.BorshCoder(reinitIdl as anchor.Idl);
  const currentAccount = coder.types.decode(
    "MyCompressedAccount",
    compressedAccount.data.data,
  );

  const compressedAccountMeta = {
    treeInfo: {
      merkleTreePubkeyIndex,
      queuePubkeyIndex,
      leafIndex: compressedAccount.leafIndex,
      proveByIndex: false,
      rootIndex: proofRpcResult.rootIndices[0],
    },
    address: compressedAccount.address,
    outputStateTreeIndex,
  };

  let proof = {
    0: proofRpcResult.compressedProof,
  };
  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });
  let tx = await program.methods
    .closeAccount(proof, compressedAccountMeta, message)
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

async function reinitCompressedAccount(
  rpc: Rpc,
  compressedAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Reinit>,
  stateTreeInfo: TreeInfo,
  signer: anchor.web3.Keypair,
) {
  const systemAccountConfig = new SystemAccountMetaConfig(program.programId);
  let remainingAccounts = new PackedAccounts();
  remainingAccounts.addSystemAccountsV2(systemAccountConfig);

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

  const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.tree,
  );
  const queuePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.queue,
  );
  const outputStateTreeIndex =
    remainingAccounts.insertOrGet(stateTreeInfo.queue);

  const compressedAccountMeta = {
    treeInfo: {
      merkleTreePubkeyIndex,
      queuePubkeyIndex,
      leafIndex: compressedAccount.leafIndex,
      proveByIndex: false,
      rootIndex: proofRpcResult.rootIndices[0],
    },
    address: compressedAccount.address,
    outputStateTreeIndex,
  };

  let proof = {
    0: proofRpcResult.compressedProof,
  };
  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });
  let tx = await program.methods
    .reinitAccount(proof, compressedAccountMeta)
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
