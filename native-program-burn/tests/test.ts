import { web3 } from "@coral-xyz/anchor";
import {
  bn,
  createRpc,
  defaultTestStateTreeAccounts,
  deriveAddress,
  deriveAddressSeed,
  LightSystemProgram,
  Rpc,
  sleep,
} from "@lightprotocol/stateless.js";
import * as borsh from "borsh";

const path = require("path");
const os = require("os");
require("dotenv").config();

const CREATE_PROGRAM_ID = new web3.PublicKey("rent4o4eAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPq");
const BURN_PROGRAM_ID = new web3.PublicKey("rent4o4eAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPq");

describe("native-program-burn", () => {
  it("create and burn compressed account", async () => {
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

    const outputMerkleTree = defaultTestStateTreeAccounts().merkleTree;
    const addressTree = defaultTestStateTreeAccounts().addressTree;
    const addressQueue = defaultTestStateTreeAccounts().addressQueue;

    const messageSeed = new TextEncoder().encode("message");
    const seed = deriveAddressSeed(
      [messageSeed, signer.publicKey.toBytes()],
      BURN_PROGRAM_ID,
    );
    const address = deriveAddress(seed, addressTree);

    // Create compressed account
    await createCompressedAccount(
      rpc,
      addressTree,
      addressQueue,
      address,
      outputMerkleTree,
      signer,
      "Hello, compressed world!",
    );
    await sleep(2000);
    console.log("Created account");

    let compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));

    // Burn the account
    await burnCompressedAccount(
      rpc,
      compressedAccount,
      signer,
      "Hello, compressed world!",
    );
    await sleep(2000);
    console.log("Burned account");

    // Account should be None after burn
    compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    console.log("Account burned, should be null:", compressedAccount);
  });
});

async function createCompressedAccount(
  rpc: Rpc,
  addressTree: web3.PublicKey,
  addressQueue: web3.PublicKey,
  address: web3.PublicKey,
  outputMerkleTree: web3.PublicKey,
  signer: web3.Keypair,
  message: string,
) {
  const proofRpcResult = await rpc.getValidityProofV0([], [{ tree: addressTree, queue: addressQueue, address: bn(address.toBytes()) }]);
  const systemAccountConfig = SystemAccountMetaConfig.new(CREATE_PROGRAM_ID);
  let remainingAccounts = PackedAccounts.newWithSystemAccounts(systemAccountConfig);

  const addressMerkleTreePubkeyIndex = remainingAccounts.insertOrGet(addressTree);
  const addressQueuePubkeyIndex = remainingAccounts.insertOrGet(addressQueue);
  const packedAddressTreeInfo = { rootIndex: proofRpcResult.rootIndices[0], addressMerkleTreePubkeyIndex, addressQueuePubkeyIndex };
  const outputMerkleTreeIndex = remainingAccounts.insertOrGet(outputMerkleTree);

  const instructionData = { proof: proofRpcResult.compressedProof, addressTreeInfo: packedAddressTreeInfo, outputStateTreeIndex: outputMerkleTreeIndex, message };
  const instructionDataSchema = new Map([
    [Object, { kind: "struct", fields: [["proof", { kind: "option", type: "CompressedProof" }], ["addressTreeInfo", "PackedAddressTreeInfo"], ["outputStateTreeIndex", "u8"], ["message", "string"]] }],
    ["CompressedProof", { kind: "struct", fields: [["a", [32]], ["b", [64]], ["c", [32]]] }],
    ["PackedAddressTreeInfo", { kind: "struct", fields: [["rootIndex", "u16"], ["addressMerkleTreePubkeyIndex", "u8"], ["addressQueuePubkeyIndex", "u8"]] }],
  ]);

  const serializedData = borsh.serialize(instructionDataSchema, instructionData);
  const instruction = new web3.TransactionInstruction({
    keys: remainingAccounts.toAccountMetas().remainingAccounts,
    programId: CREATE_PROGRAM_ID,
    data: Buffer.concat([Buffer.from([0]), Buffer.from(serializedData)]),
  });

  let tx = new web3.Transaction();
  tx.add(web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 1000000 }));
  tx.add(instruction);
  tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
  tx.feePayer = signer.publicKey;
  tx.sign(signer);

  await rpc.confirmTransaction(await rpc.sendTransaction(tx, [signer]));
}

async function burnCompressedAccount(rpc: Rpc, compressedAccount: any, signer: web3.Keypair, currentMessage: string) {
  const proofRpcResult = await rpc.getValidityProofV0([bn(compressedAccount.hash)], []);
  const systemAccountConfig = SystemAccountMetaConfig.new(BURN_PROGRAM_ID);
  let remainingAccounts = PackedAccounts.newWithSystemAccounts(systemAccountConfig);

  const stateMerkleTreePubkey = new web3.PublicKey(compressedAccount.merkleTree);
  const stateMerkleTreeIndex = remainingAccounts.insertOrGet(stateMerkleTreePubkey);

  // Note: Burn uses CompressedAccountMetaBurn which doesn't have output_state_tree_index
  const accountMeta = { treeInfo: { rootIndex: proofRpcResult.rootIndices[0], treeIndex: stateMerkleTreeIndex }, address: Array.from(compressedAccount.address) };
  const instructionData = { proof: proofRpcResult.compressedProof, accountMeta, currentMessage };

  const instructionDataSchema = new Map([
    [Object, { kind: "struct", fields: [["proof", { kind: "option", type: "CompressedProof" }], ["accountMeta", "CompressedAccountMetaBurn"], ["currentMessage", "string"]] }],
    ["CompressedProof", { kind: "struct", fields: [["a", [32]], ["b", [64]], ["c", [32]]] }],
    ["CompressedAccountMetaBurn", { kind: "struct", fields: [["treeInfo", "PackedTreeInfo"], ["address", [32]]] }],
    ["PackedTreeInfo", { kind: "struct", fields: [["rootIndex", "u16"], ["treeIndex", "u8"]] }],
  ]);

  const serializedData = borsh.serialize(instructionDataSchema, instructionData);
  const instruction = new web3.TransactionInstruction({
    keys: remainingAccounts.toAccountMetas().remainingAccounts,
    programId: BURN_PROGRAM_ID,
    data: Buffer.concat([Buffer.from([0]), Buffer.from(serializedData)]),
  });

  let tx = new web3.Transaction();
  tx.add(web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 1000000 }));
  tx.add(instruction);
  tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
  tx.feePayer = signer.publicKey;
  tx.sign(signer);

  await rpc.confirmTransaction(await rpc.sendTransaction(tx, [signer]));
}

class PackedAccounts {
  private preAccounts: web3.AccountMeta[] = [];
  private systemAccounts: web3.AccountMeta[] = [];
  private nextIndex: number = 0;
  private map: Map<string, [number, web3.AccountMeta]> = new Map();

  static newWithSystemAccounts(config: SystemAccountMetaConfig): PackedAccounts {
    const instance = new PackedAccounts();
    instance.addSystemAccounts(config);
    return instance;
  }

  addSystemAccounts(config: SystemAccountMetaConfig): void {
    this.systemAccounts.push(...getLightSystemAccountMetas(config));
  }

  insertOrGet(pubkey: web3.PublicKey): number {
    const key = pubkey.toBase58();
    const entry = this.map.get(key);
    if (entry) return entry[0];
    const index = this.nextIndex++;
    const meta: web3.AccountMeta = { pubkey, isSigner: false, isWritable: true };
    this.map.set(key, [index, meta]);
    return index;
  }

  toAccountMetas(): { remainingAccounts: web3.AccountMeta[]; systemStart: number; packedStart: number } {
    const entries = Array.from(this.map.entries());
    entries.sort((a, b) => a[1][0] - b[1][0]);
    const packed = entries.map(([, [, meta]]) => meta);
    return {
      remainingAccounts: [...this.preAccounts, ...this.systemAccounts, ...packed],
      systemStart: this.preAccounts.length,
      packedStart: this.preAccounts.length + this.systemAccounts.length,
    };
  }
}

class SystemAccountMetaConfig {
  selfProgram: web3.PublicKey;
  private constructor(selfProgram: web3.PublicKey) { this.selfProgram = selfProgram; }
  static new(selfProgram: web3.PublicKey): SystemAccountMetaConfig { return new SystemAccountMetaConfig(selfProgram); }
}

function getLightSystemAccountMetas(config: SystemAccountMetaConfig): web3.AccountMeta[] {
  const cpiSigner = web3.PublicKey.findProgramAddressSync([new TextEncoder().encode("cpi_authority")], config.selfProgram)[0];
  const defaults = {
    lightSystemProgram: LightSystemProgram.programId,
    systemProgram: web3.PublicKey.default,
    accountCompressionProgram: new web3.PublicKey("CbjvJc1SNx1aav8tU49dJGHu8EUdzQJSMtkjDmV8miqK"),
    accountCompressionAuthority: new web3.PublicKey("GBP8yM8xj7Ls1rMaFTLLHm1MHf1WL1f5NKdqXYRLKaGz"),
    registeredProgramPda: new web3.PublicKey("Gvnx929TW5SNWHCNc1N1F23fTJ2dELG9Y8D39fmWWGVb"),
    noopProgram: new web3.PublicKey("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV"),
  };
  return [
    { pubkey: defaults.lightSystemProgram, isSigner: false, isWritable: false },
    { pubkey: cpiSigner, isSigner: false, isWritable: false },
    { pubkey: defaults.registeredProgramPda, isSigner: false, isWritable: false },
    { pubkey: defaults.noopProgram, isSigner: false, isWritable: false },
    { pubkey: defaults.accountCompressionAuthority, isSigner: false, isWritable: false },
    { pubkey: defaults.accountCompressionProgram, isSigner: false, isWritable: false },
    { pubkey: config.selfProgram, isSigner: false, isWritable: false },
    { pubkey: defaults.systemProgram, isSigner: false, isWritable: false },
  ];
}
