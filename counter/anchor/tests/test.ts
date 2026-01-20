import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { Counter } from "../target/types/counter";
import idl from "../target/idl/counter.json";
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
import { assert } from "chai";

// Force V2 mode by overwriting the featureFlags
(featureFlags as any).version = VERSION.V2;
const path = require("path");
const os = require("os");
require("dotenv").config();

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-anchor", () => {
  const program = anchor.workspace.Counter as Program<Counter>;
  const coder = new anchor.BorshCoder(idl as anchor.Idl);

  it("", async () => {
    let signer = new web3.Keypair();
    let rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      {
        commitment: "confirmed",
      }
    );
    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(signer.publicKey, lamports);
    await sleep(2000);

    const stateTreeInfos = await rpc.getStateTreeInfos();
    const stateTreeInfo = selectStateTreeInfo(stateTreeInfos);
    const addressTree = new web3.PublicKey(batchAddressTree);

    const counterSeed = new TextEncoder().encode("counter");
    const seed = deriveAddressSeedV2([counterSeed, signer.publicKey.toBytes()]);
    const address = deriveAddressV2(
      seed,
      addressTree,
      new web3.PublicKey(program.idl.address)
    );
    // Create counter compressed account.
    await CreateCounterCompressedAccount(
      rpc,
      addressTree,
      address,
      program,
      stateTreeInfo,
      signer
    );

    let counterAccount = await rpc.getCompressedAccount(bn(address.toBytes()));

    let counter = coder.types.decode(
      "CounterAccount",
      counterAccount.data.data
    );
    console.log("counter account ", counterAccount);
    console.log("des counter ", counter);

    await incrementCounterCompressedAccount(
      rpc,
      counter.value,
      counterAccount,
      program,
      stateTreeInfo,
      signer
    );

    counterAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    counter = coder.types.decode("CounterAccount", counterAccount.data.data);
    console.log("counter account ", counterAccount);
    console.log("des counter ", counter);

    await deleteCounterCompressedAccount(
      rpc,
      counter.value,
      counterAccount,
      program,
      stateTreeInfo,
      signer
    );

    const deletedCounterAccount = await rpc.getCompressedAccount(
      bn(address.toBytes())
    );
    console.log("deletedCounterAccount ", deletedCounterAccount);

    assert.isTrue(deletedCounterAccount.data.data.length === 0);
    assert.equal(
      deletedCounterAccount.data.discriminator.toString(),
      Array(8).fill(0).toString()
    );
    assert.equal(
      deletedCounterAccount.data.dataHash.toString(),
      Array(32).fill(0).toString()
    );
  });
});

async function CreateCounterCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Counter>,
  stateTreeInfo: TreeInfo,
  signer: anchor.web3.Keypair
) {
  {
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
    let remainingAccounts =
      PackedAccounts.newWithSystemAccountsV2(systemAccountConfig);

    const addressMerkleTreePubkeyIndex =
      remainingAccounts.insertOrGet(addressTree);
    const addressQueuePubkeyIndex = addressMerkleTreePubkeyIndex;
    const packedAddreesMerkleContext = {
      rootIndex: proofRpcResult.rootIndices[0],
      addressMerkleTreePubkeyIndex,
      addressQueuePubkeyIndex,
    };
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(stateTreeInfo.queue);

    let proof = {
      0: proofRpcResult.compressedProof,
    };
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });
    let tx = await program.methods
      .createCounter(proof, packedAddreesMerkleContext, outputMerkleTreeIndex)
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

    console.log("Created counter compressed account ", sig);
  }
}

async function incrementCounterCompressedAccount(
  rpc: Rpc,
  counterValue: anchor.BN,
  counterAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Counter>,
  stateTreeInfo: TreeInfo,
  signer: anchor.web3.Keypair
) {
  {
    const proofRpcResult = await rpc.getValidityProofV0(
      [
        {
          hash: counterAccount.hash,
          tree: counterAccount.treeInfo.tree,
          queue: counterAccount.treeInfo.queue,
        },
      ],
      []
    );
    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccountsV2(systemAccountConfig);

    const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.tree
    );
    const queuePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.queue
    );
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(stateTreeInfo.queue);
    const compressedAccountMeta = {
      treeInfo: {
        rootIndex: proofRpcResult.rootIndices[0],
        proveByIndex: false,
        merkleTreePubkeyIndex,
        queuePubkeyIndex,
        leafIndex: counterAccount.leafIndex,
      },
      address: counterAccount.address,
      outputStateTreeIndex: outputMerkleTreeIndex,
    };

    let proof = {
      0: proofRpcResult.compressedProof,
    };
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });
    let tx = await program.methods
      .incrementCounter(proof, counterValue, compressedAccountMeta)
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

    console.log("Incremented counter compressed account ", sig);
  }
}

async function deleteCounterCompressedAccount(
  rpc: Rpc,
  counterValue: anchor.BN,
  counterAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Counter>,
  stateTreeInfo: TreeInfo,
  signer: anchor.web3.Keypair
) {
  {
    const proofRpcResult = await rpc.getValidityProofV0(
      [
        {
          hash: counterAccount.hash,
          tree: counterAccount.treeInfo.tree,
          queue: counterAccount.treeInfo.queue,
        },
      ],
      []
    );
    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccountsV2(systemAccountConfig);

    const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.tree
    );
    const queuePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.queue
    );
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(stateTreeInfo.queue);

    const compressedAccountMeta = {
      treeInfo: {
        rootIndex: proofRpcResult.rootIndices[0],
        proveByIndex: false,
        merkleTreePubkeyIndex,
        queuePubkeyIndex,
        leafIndex: counterAccount.leafIndex,
      },
      address: counterAccount.address,
      outputStateTreeIndex: outputMerkleTreeIndex,
    };

    let proof = {
      0: proofRpcResult.compressedProof,
    };
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });
    let tx = await program.methods
      .closeCounter(proof, counterValue, compressedAccountMeta)
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

    console.log("Deleted counter compressed account ", sig);
  }
}
