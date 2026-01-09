import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { Create } from "../target/types/create";
import idl from "../target/idl/create.json";
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

describe("test-anchor", () => {
  const program = anchor.workspace.Create as Program<Create>;
  const coder = new anchor.BorshCoder(idl as anchor.Idl);

  it("create compressed account", async () => {
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
      new web3.PublicKey(program.idl.address),
    );

    // Create compressed account with message
    const txId = await createCompressedAccount(
      rpc,
      addressTree,
      address,
      program,
      outputStateTree,
      signer,
      "Hello, compressed world!",
    );
    console.log("Transaction ID:", txId);

    // Wait for indexer to process the transaction
    const slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    let compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    let myAccount = coder.types.decode(
      "MyCompressedAccount",
      compressedAccount.data.data,
    );

    console.log("Decoded data owner:", myAccount.owner.toBase58());
    console.log("Decoded data message:", myAccount.message);

    // Verify account data
    assert.ok(
      myAccount.owner.equals(signer.publicKey),
      "Owner should match signer public key"
    );
    assert.strictEqual(
      myAccount.message,
      "Hello, compressed world!",
      "Message should match the created message"
    );
  });
});

async function createCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Create>,
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
