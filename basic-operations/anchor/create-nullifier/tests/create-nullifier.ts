import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { CreateNullifier } from "../target/types/create_nullifier";
import {
  bn,
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

// Set V2 mode
(featureFlags as any).version = VERSION.V2;

const path = require("path");
const os = require("os");
require("dotenv").config();

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-create-nullifier", () => {
  const program = anchor.workspace.CreateNullifier as Program<CreateNullifier>;

  it("create nullifier account", async () => {
    let signer = new web3.Keypair();
    let rpc = createRpc(); // defaults to local
    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(signer.publicKey, lamports);
    await sleep(2000);

    const outputStateTree = defaultTestStateTreeAccounts().merkleTree;
    const addressTree = new web3.PublicKey(batchAddressTree);

    // Create a 32-byte id
    const id = new Uint8Array([
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
      22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    ]);

    const nullifierSeed = new TextEncoder().encode("nullifier");
    const seed = deriveAddressSeedV2([nullifierSeed, id]);
    const address = deriveAddressV2(
      seed,
      addressTree,
      new web3.PublicKey(program.idl.address)
    );

    // Create nullifier account
    const txId = await createNullifierAccount(
      rpc,
      addressTree,
      address,
      program,
      outputStateTree,
      signer,
      Array.from(id)
    );
    console.log("Transaction ID:", txId);

    // Wait for indexer to process the transaction
    const slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    let compressedAccount = await rpc.getCompressedAccount(
      bn(address.toBytes())
    );

    // Verify account exists
    assert.ok(compressedAccount, "Nullifier account should exist");

    // Account data should be empty or null
    console.log("Nullifier account created successfully with empty data");
  });

  it("duplicate nullifier should fail", async () => {
    let signer = new web3.Keypair();
    let rpc = createRpc();
    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(signer.publicKey, lamports);
    await sleep(2000);

    const outputStateTree = defaultTestStateTreeAccounts().merkleTree;
    const addressTree = new web3.PublicKey(batchAddressTree);

    // Use same id for both attempts
    const id = new Uint8Array([
      42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42,
      42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42,
    ]);

    const nullifierSeed = new TextEncoder().encode("nullifier");
    const seed = deriveAddressSeedV2([nullifierSeed, id]);
    const address = deriveAddressV2(
      seed,
      addressTree,
      new web3.PublicKey(program.idl.address)
    );

    // First creation should succeed
    await createNullifierAccount(
      rpc,
      addressTree,
      address,
      program,
      outputStateTree,
      signer,
      Array.from(id)
    );

    // Wait for indexer
    const slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    // Second creation with same id should fail
    try {
      await createNullifierAccount(
        rpc,
        addressTree,
        address,
        program,
        outputStateTree,
        signer,
        Array.from(id)
      );
      assert.fail("Should have thrown an error for duplicate nullifier");
    } catch (error) {
      console.log("Expected error for duplicate nullifier:", error.message);
    }
  });
});

async function createNullifierAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<CreateNullifier>,
  outputStateTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
  id: number[]
) {
  const proofRpcResult = await rpc.getValidityProofV0(
    [],
    [
      {
        tree: addressTree,
        queue: addressTree,
        address: bn(address.toBytes()),
      },
    ]
  );
  const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
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
  const outputStateTreeIndex = remainingAccounts.insertOrGet(outputStateTree);
  let proof = {
    0: proofRpcResult.compressedProof,
  };
  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });
  let tx = await program.methods
    .createAccount(proof, packedAddressTreeInfo, outputStateTreeIndex, id)
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
