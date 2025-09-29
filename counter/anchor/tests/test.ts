import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { Counter } from "../target/types/counter";
import idl from "../target/idl/counter.json";
import {
  bn,
  CompressedAccountWithMerkleContext,
  createRpc,
  defaultStaticAccountsStruct,
  defaultTestStateTreeAccounts,
  deriveAddress,
  deriveAddressSeed,
  LightSystemProgram,
  Rpc,
  sleep,
  featureFlags,
  batchMerkleTree,
  batchQueue,
} from "@lightprotocol/stateless.js";
import { keccak_256 } from "@noble/hashes/sha3";
const path = require("path");
const os = require("os");
require("dotenv").config();

// Set Light Protocol to V2
// process.env.LIGHT_PROTOCOL_VERSION = "V2";

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

    // Check if V2 is enabled
    console.log("V2 enabled:", featureFlags.isV2());
    console.log("Feature flags version:", featureFlags.version);

    // Get existing tree infos to check what's available
    const existingTreeInfos = await rpc.getStateTreeInfos();
    console.log("Available tree infos:");
    existingTreeInfos.forEach((info) => {
      console.log(`  Tree: ${info.tree.toBase58()}, Type: ${info.treeType}`);
    });

    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(signer.publicKey, lamports);
    await sleep(2000);

    const outputQueue = new web3.PublicKey(batchQueue);
    const outputMerkleTree = new web3.PublicKey(batchMerkleTree);
    const addressTree = new web3.PublicKey(
      "EzKE84aVTkCUhDHLELqyJaq1Y7UVVmqxXqZjVHwHY3rK"
    );
    const addressQueue = new web3.PublicKey(
      "EzKE84aVTkCUhDHLELqyJaq1Y7UVVmqxXqZjVHwHY3rK"
    );

    const counterSeed = new TextEncoder().encode("counter");
    const seed = deriveAddressSeedV2([counterSeed, signer.publicKey.toBytes()]);
    console.log("seed ", Array.from(seed));
    const address = deriveAddressV2(
      seed,
      addressTree,
      new web3.PublicKey(program.idl.address)
    );
    console.log("address ", Array.from(address.toBytes()));
    // Create counter compressed account.
    await CreateCounterCompressedAccount(
      rpc,
      addressTree,
      addressQueue,
      address,
      program,
      outputQueue,
      signer
    );
    // Wait for indexer to catch up.
    await sleep(2000);

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
      outputQueue,
      signer
    );

    // Wait for indexer to catch up.
    await sleep(2000);

    counterAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    counter = coder.types.decode("CounterAccount", counterAccount.data.data);
    console.log("counter account ", counterAccount);
    console.log("des counter ", counter);

    await deleteCounterCompressedAccount(
      rpc,
      counter.value,
      counterAccount,
      program,
      outputQueue,
      signer
    );

    // Wait for indexer to catch up.
    await sleep(2000);

    const deletedCounterAccount = await rpc.getCompressedAccount(
      bn(address.toBytes())
    );
    console.log("deletedCounterAccount ", deletedCounterAccount);
  });
});

async function CreateCounterCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  addressQueue: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Counter>,
  outputMerkleTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair
) {
  {
    const proofRpcResult = await rpc.getValidityProofV0(
      [],
      [
        {
          tree: addressTree,
          queue: addressQueue,
          address: bn(address.toBytes()),
        },
      ]
    );
    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    const addressMerkleTreePubkeyIndex =
      remainingAccounts.insertOrGet(addressTree);
    const addressQueuePubkeyIndex = remainingAccounts.insertOrGet(addressQueue);
    const packedAddreesMerkleContext = {
      rootIndex: proofRpcResult.rootIndices[0],
      addressMerkleTreePubkeyIndex,
      addressQueuePubkeyIndex,
    };
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(outputMerkleTree);

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
    await rpc.confirmTransaction(sig);
    console.log("Created counter compressed account ", sig);
  }
}

async function incrementCounterCompressedAccount(
  rpc: Rpc,
  counterValue: anchor.BN,
  counterAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Counter>,
  outputMerkleTree: anchor.web3.PublicKey,
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
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.tree
    );
    const queuePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.queue
    );
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(outputMerkleTree);
    const compressedAccountMeta = {
      treeInfo: {
        rootIndex: proofRpcResult.rootIndices[0],
        proveByIndex: proofRpcResult.proveByIndices[0],
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
    await rpc.confirmTransaction(sig);
    console.log("Incremented counter compressed account ", sig);
  }
}

async function deleteCounterCompressedAccount(
  rpc: Rpc,
  counterValue: anchor.BN,
  counterAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Counter>,
  outputMerkleTree: anchor.web3.PublicKey,
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
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.tree
    );
    const queuePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.queue
    );
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(outputMerkleTree);

    const compressedAccountMeta = {
      treeInfo: {
        rootIndex: proofRpcResult.rootIndices[0],
        proveByIndex: proofRpcResult.proveByIndices[0],
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
    await rpc.confirmTransaction(sig);
    console.log("Deleted counter compressed account ", sig);
  }
}

// TODO: import
class PackedAccounts {
  private preAccounts: web3.AccountMeta[] = [];
  private systemAccounts: web3.AccountMeta[] = [];
  private nextIndex: number = 0;
  private map: Map<web3.PublicKey, [number, web3.AccountMeta]> = new Map();

  static newWithSystemAccounts(
    config: SystemAccountMetaConfig
  ): PackedAccounts {
    const instance = new PackedAccounts();
    instance.addSystemAccounts(config);
    return instance;
  }

  addPreAccountsSigner(pubkey: web3.PublicKey): void {
    this.preAccounts.push({ pubkey, isSigner: true, isWritable: false });
  }

  addPreAccountsSignerMut(pubkey: web3.PublicKey): void {
    this.preAccounts.push({ pubkey, isSigner: true, isWritable: true });
  }

  addPreAccountsMeta(accountMeta: web3.AccountMeta): void {
    this.preAccounts.push(accountMeta);
  }

  addSystemAccounts(config: SystemAccountMetaConfig): void {
    this.systemAccounts.push(...getLightSystemAccountMetasV2(config));
  }

  insertOrGet(pubkey: web3.PublicKey): number {
    return this.insertOrGetConfig(pubkey, false, true);
  }

  insertOrGetReadOnly(pubkey: web3.PublicKey): number {
    return this.insertOrGetConfig(pubkey, false, false);
  }

  insertOrGetConfig(
    pubkey: web3.PublicKey,
    isSigner: boolean,
    isWritable: boolean
  ): number {
    const entry = this.map.get(pubkey);
    if (entry) {
      return entry[0];
    }
    const index = this.nextIndex++;
    const meta: web3.AccountMeta = { pubkey, isSigner, isWritable };
    this.map.set(pubkey, [index, meta]);
    return index;
  }

  private hashSetAccountsToMetas(): web3.AccountMeta[] {
    const entries = Array.from(this.map.entries());
    entries.sort((a, b) => a[1][0] - b[1][0]);
    return entries.map(([, [, meta]]) => meta);
  }

  private getOffsets(): [number, number] {
    const systemStart = this.preAccounts.length;
    const packedStart = systemStart + this.systemAccounts.length;
    return [systemStart, packedStart];
  }

  toAccountMetas(): {
    remainingAccounts: web3.AccountMeta[];
    systemStart: number;
    packedStart: number;
  } {
    const packed = this.hashSetAccountsToMetas();
    const [systemStart, packedStart] = this.getOffsets();
    return {
      remainingAccounts: [
        ...this.preAccounts,
        ...this.systemAccounts,
        ...packed,
      ],
      systemStart,
      packedStart,
    };
  }
}

// TODO: import
class SystemAccountMetaConfig {
  selfProgram: web3.PublicKey;
  cpiContext?: web3.PublicKey;
  solCompressionRecipient?: web3.PublicKey;
  solPoolPda?: web3.PublicKey;

  private constructor(
    selfProgram: web3.PublicKey,
    cpiContext?: web3.PublicKey,
    solCompressionRecipient?: web3.PublicKey,
    solPoolPda?: web3.PublicKey
  ) {
    this.selfProgram = selfProgram;
    this.cpiContext = cpiContext;
    this.solCompressionRecipient = solCompressionRecipient;
    this.solPoolPda = solPoolPda;
  }

  static new(selfProgram: web3.PublicKey): SystemAccountMetaConfig {
    return new SystemAccountMetaConfig(selfProgram);
  }

  static newWithCpiContext(
    selfProgram: web3.PublicKey,
    cpiContext: web3.PublicKey
  ): SystemAccountMetaConfig {
    return new SystemAccountMetaConfig(selfProgram, cpiContext);
  }
}

// TODO: import
function getLightSystemAccountMetasV2(
  config: SystemAccountMetaConfig
): web3.AccountMeta[] {
  let signerSeed = new TextEncoder().encode("cpi_authority");
  const cpiSigner = web3.PublicKey.findProgramAddressSync(
    [signerSeed],
    config.selfProgram
  )[0];
  const defaults = SystemAccountPubkeys.default();
  const metas: web3.AccountMeta[] = [
    { pubkey: defaults.lightSystemProgram, isSigner: false, isWritable: false },
    { pubkey: cpiSigner, isSigner: false, isWritable: false },
    {
      pubkey: defaults.registeredProgramPda,
      isSigner: false,
      isWritable: false,
    },
    {
      pubkey: defaults.accountCompressionAuthority,
      isSigner: false,
      isWritable: false,
    },
    {
      pubkey: defaults.accountCompressionProgram,
      isSigner: false,
      isWritable: false,
    },
  ];
  if (config.solPoolPda) {
    metas.push({
      pubkey: config.solPoolPda,
      isSigner: false,
      isWritable: true,
    });
  }
  if (config.solCompressionRecipient) {
    metas.push({
      pubkey: config.solCompressionRecipient,
      isSigner: false,
      isWritable: true,
    });
  }
  metas.push({
    pubkey: defaults.systemProgram,
    isSigner: false,
    isWritable: false,
  });
  if (config.cpiContext) {
    metas.push({
      pubkey: config.cpiContext,
      isSigner: false,
      isWritable: true,
    });
  }
  return metas;
}

// TODO: import
class SystemAccountPubkeys {
  lightSystemProgram: web3.PublicKey;
  systemProgram: web3.PublicKey;
  accountCompressionProgram: web3.PublicKey;
  accountCompressionAuthority: web3.PublicKey;
  registeredProgramPda: web3.PublicKey;
  solPoolPda: web3.PublicKey;

  private constructor(
    lightSystemProgram: web3.PublicKey,
    systemProgram: web3.PublicKey,
    accountCompressionProgram: web3.PublicKey,
    accountCompressionAuthority: web3.PublicKey,
    registeredProgramPda: web3.PublicKey,
    solPoolPda: web3.PublicKey
  ) {
    this.lightSystemProgram = lightSystemProgram;
    this.systemProgram = systemProgram;
    this.accountCompressionProgram = accountCompressionProgram;
    this.accountCompressionAuthority = accountCompressionAuthority;
    this.registeredProgramPda = registeredProgramPda;
    this.solPoolPda = solPoolPda;
  }

  static default(): SystemAccountPubkeys {
    return new SystemAccountPubkeys(
      LightSystemProgram.programId,
      web3.PublicKey.default,
      defaultStaticAccountsStruct().accountCompressionProgram,
      defaultStaticAccountsStruct().accountCompressionAuthority,
      defaultStaticAccountsStruct().registeredProgramPda,
      web3.PublicKey.default
    );
  }
}
// TODO: import
function deriveAddressSeedV2(seeds: Uint8Array[]): Uint8Array {
  const combinedSeeds: Uint8Array[] = seeds.map((seed) =>
    Uint8Array.from(seed)
  );
  const hash = hashvToBn254FieldSizeBeU8Array(combinedSeeds);
  return hash;
}

/**
 * Derives an address from a seed using the v2 method (matching Rust's derive_address_from_seed)
 *
 * @param addressSeed              The address seed (32 bytes)
 * @param addressMerkleTreePubkey  Merkle tree public key
 * @param programId                Program ID
 * @returns                        Derived address
 */
// TODO: import
function deriveAddressV2(
  addressSeed: Uint8Array,
  addressMerkleTreePubkey: web3.PublicKey,
  programId: web3.PublicKey
): web3.PublicKey {
  if (addressSeed.length != 32) {
    throw new Error("Address seed length is not 32 bytes.");
  }
  const merkleTreeBytes = addressMerkleTreePubkey.toBytes();
  const programIdBytes = programId.toBytes();
  // Match Rust implementation: hash [seed, merkle_tree_pubkey, program_id]
  const combined = [
    Uint8Array.from(addressSeed),
    Uint8Array.from(merkleTreeBytes),
    Uint8Array.from(programIdBytes),
  ];
  const hash = hashvToBn254FieldSizeBeU8Array(combined);
  return new web3.PublicKey(hash);
}
// TODO: import
function hashvToBn254FieldSizeBeU8Array(bytes: Uint8Array[]): Uint8Array {
  const hasher = keccak_256.create();
  for (const input of bytes) {
    hasher.update(input);
  }
  hasher.update(Uint8Array.from([255]));
  const hash = hasher.digest();
  hash[0] = 0;
  return hash;
}
