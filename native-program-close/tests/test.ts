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
const CLOSE_PROGRAM_ID = new web3.PublicKey("rent4o4eAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPq");

class MyCompressedAccount {
  owner: Uint8Array;
  message: string;

  constructor(fields: { owner: Uint8Array; message: string }) {
    this.owner = fields.owner;
    this.message = fields.message;
  }

  static schema = new Map([
    [
      MyCompressedAccount,
      {
        kind: "struct",
        fields: [
          ["owner", [32]],
          ["message", "string"],
        ],
      },
    ],
  ]);
}

describe("native-program-close", () => {
  it("create and close compressed account", async () => {
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
      CLOSE_PROGRAM_ID,
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

    // Wait for indexer to catch up
    await sleep(2000);

    let compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    console.log("Created account");

    // Close the account
    await closeCompressedAccount(
      rpc,
      compressedAccount,
      outputMerkleTree,
      signer,
      "Hello, compressed world!",
    );

    // Wait for indexer to catch up
    await sleep(2000);

    compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    console.log("Account closed, data should be default:", compressedAccount.data);
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
  const proofRpcResult = await rpc.getValidityProofV0(
    [],
    [
      {
        tree: addressTree,
        queue: addressQueue,
        address: bn(address.toBytes()),
      },
    ],
  );
  const systemAccountConfig = SystemAccountMetaConfig.new(CREATE_PROGRAM_ID);
  let remainingAccounts =
    PackedAccounts.newWithSystemAccounts(systemAccountConfig);

  const addressMerkleTreePubkeyIndex =
    remainingAccounts.insertOrGet(addressTree);
  const addressQueuePubkeyIndex = remainingAccounts.insertOrGet(addressQueue);
  const packedAddressTreeInfo = {
    rootIndex: proofRpcResult.rootIndices[0],
    addressMerkleTreePubkeyIndex,
    addressQueuePubkeyIndex,
  };
  const outputMerkleTreeIndex =
    remainingAccounts.insertOrGet(outputMerkleTree);

  const instructionData = {
    proof: proofRpcResult.compressedProof,
    addressTreeInfo: packedAddressTreeInfo,
    outputStateTreeIndex: outputMerkleTreeIndex,
    message: message,
  };

  const instructionDataSchema = new Map([
    [
      Object,
      {
        kind: "struct",
        fields: [
          ["proof", { kind: "option", type: "CompressedProof" }],
          ["addressTreeInfo", "PackedAddressTreeInfo"],
          ["outputStateTreeIndex", "u8"],
          ["message", "string"],
        ],
      },
    ],
    [
      "CompressedProof",
      {
        kind: "struct",
        fields: [
          ["a", [32]],
          ["b", [64]],
          ["c", [32]],
        ],
      },
    ],
    [
      "PackedAddressTreeInfo",
      {
        kind: "struct",
        fields: [
          ["rootIndex", "u16"],
          ["addressMerkleTreePubkeyIndex", "u8"],
          ["addressQueuePubkeyIndex", "u8"],
        ],
      },
    ],
  ]);

  const serializedData = borsh.serialize(instructionDataSchema, instructionData);
  const instructionDiscriminator = Buffer.from([0]); // InstructionType::Create = 0

  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });

  const instruction = new web3.TransactionInstruction({
    keys: remainingAccounts.toAccountMetas().remainingAccounts,
    programId: CREATE_PROGRAM_ID,
    data: Buffer.concat([instructionDiscriminator, Buffer.from(serializedData)]),
  });

  let tx = new web3.Transaction();
  tx.add(computeBudgetIx);
  tx.add(instruction);
  tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
  tx.feePayer = signer.publicKey;
  tx.sign(signer);

  const sig = await rpc.sendTransaction(tx, [signer]);
  await rpc.confirmTransaction(sig);
  console.log("Created compressed account: ", sig);
}

async function closeCompressedAccount(
  rpc: Rpc,
  compressedAccount: any,
  outputMerkleTree: web3.PublicKey,
  signer: web3.Keypair,
  currentMessage: string,
) {
  const proofRpcResult = await rpc.getValidityProofV0(
    [bn(compressedAccount.hash)],
    [],
  );

  const systemAccountConfig = SystemAccountMetaConfig.new(CLOSE_PROGRAM_ID);
  let remainingAccounts =
    PackedAccounts.newWithSystemAccounts(systemAccountConfig);

  const stateMerkleTreePubkey = new web3.PublicKey(
    compressedAccount.merkleTree,
  );
  const stateMerkleTreeIndex = remainingAccounts.insertOrGet(stateMerkleTreePubkey);
  const outputMerkleTreeIndex = remainingAccounts.insertOrGet(outputMerkleTree);

  const accountMeta = {
    treeInfo: {
      rootIndex: proofRpcResult.rootIndices[0],
      treeIndex: stateMerkleTreeIndex,
    },
    address: Array.from(compressedAccount.address),
    outputStateTreeIndex: outputMerkleTreeIndex,
  };

  const instructionData = {
    proof: proofRpcResult.compressedProof,
    accountMeta: accountMeta,
    currentMessage: currentMessage,
  };

  const instructionDataSchema = new Map([
    [
      Object,
      {
        kind: "struct",
        fields: [
          ["proof", { kind: "option", type: "CompressedProof" }],
          ["accountMeta", "CompressedAccountMeta"],
          ["currentMessage", "string"],
        ],
      },
    ],
    [
      "CompressedProof",
      {
        kind: "struct",
        fields: [
          ["a", [32]],
          ["b", [64]],
          ["c", [32]],
        ],
      },
    ],
    [
      "CompressedAccountMeta",
      {
        kind: "struct",
        fields: [
          ["treeInfo", "PackedTreeInfo"],
          ["address", [32]],
          ["outputStateTreeIndex", "u8"],
        ],
      },
    ],
    [
      "PackedTreeInfo",
      {
        kind: "struct",
        fields: [
          ["rootIndex", "u16"],
          ["treeIndex", "u8"],
        ],
      },
    ],
  ]);

  const serializedData = borsh.serialize(instructionDataSchema, instructionData);
  const instructionDiscriminator = Buffer.from([0]); // InstructionType::Close = 0

  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });

  const instruction = new web3.TransactionInstruction({
    keys: remainingAccounts.toAccountMetas().remainingAccounts,
    programId: CLOSE_PROGRAM_ID,
    data: Buffer.concat([instructionDiscriminator, Buffer.from(serializedData)]),
  });

  let tx = new web3.Transaction();
  tx.add(computeBudgetIx);
  tx.add(instruction);
  tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
  tx.feePayer = signer.publicKey;
  tx.sign(signer);

  const sig = await rpc.sendTransaction(tx, [signer]);
  await rpc.confirmTransaction(sig);
  console.log("Closed compressed account: ", sig);
}

class PackedAccounts {
  private preAccounts: web3.AccountMeta[] = [];
  private systemAccounts: web3.AccountMeta[] = [];
  private nextIndex: number = 0;
  private map: Map<string, [number, web3.AccountMeta]> = new Map();

  static newWithSystemAccounts(
    config: SystemAccountMetaConfig,
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
    this.systemAccounts.push(...getLightSystemAccountMetas(config));
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
    isWritable: boolean,
  ): number {
    const key = pubkey.toBase58();
    const entry = this.map.get(key);
    if (entry) {
      return entry[0];
    }
    const index = this.nextIndex++;
    const meta: web3.AccountMeta = { pubkey, isSigner, isWritable };
    this.map.set(key, [index, meta]);
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

class SystemAccountMetaConfig {
  selfProgram: web3.PublicKey;
  cpiContext?: web3.PublicKey;
  solCompressionRecipient?: web3.PublicKey;
  solPoolPda?: web3.PublicKey;

  private constructor(
    selfProgram: web3.PublicKey,
    cpiContext?: web3.PublicKey,
    solCompressionRecipient?: web3.PublicKey,
    solPoolPda?: web3.PublicKey,
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
    cpiContext: web3.PublicKey,
  ): SystemAccountMetaConfig {
    return new SystemAccountMetaConfig(selfProgram, cpiContext);
  }
}

function getLightSystemAccountMetas(
  config: SystemAccountMetaConfig,
): web3.AccountMeta[] {
  let signerSeed = new TextEncoder().encode("cpi_authority");
  const cpiSigner = web3.PublicKey.findProgramAddressSync(
    [signerSeed],
    config.selfProgram,
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
    { pubkey: defaults.noopProgram, isSigner: false, isWritable: false },
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
    { pubkey: config.selfProgram, isSigner: false, isWritable: false },
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

class SystemAccountPubkeys {
  lightSystemProgram: web3.PublicKey;
  systemProgram: web3.PublicKey;
  accountCompressionProgram: web3.PublicKey;
  accountCompressionAuthority: web3.PublicKey;
  registeredProgramPda: web3.PublicKey;
  noopProgram: web3.PublicKey;
  solPoolPda: web3.PublicKey;

  private constructor(
    lightSystemProgram: web3.PublicKey,
    systemProgram: web3.PublicKey,
    accountCompressionProgram: web3.PublicKey,
    accountCompressionAuthority: web3.PublicKey,
    registeredProgramPda: web3.PublicKey,
    noopProgram: web3.PublicKey,
    solPoolPda: web3.PublicKey,
  ) {
    this.lightSystemProgram = lightSystemProgram;
    this.systemProgram = systemProgram;
    this.accountCompressionProgram = accountCompressionProgram;
    this.accountCompressionAuthority = accountCompressionAuthority;
    this.registeredProgramPda = registeredProgramPda;
    this.noopProgram = noopProgram;
    this.solPoolPda = solPoolPda;
  }

  static default(): SystemAccountPubkeys {
    const defaults = {
      lightSystemProgram: LightSystemProgram.programId,
      systemProgram: web3.PublicKey.default,
      accountCompressionProgram: new web3.PublicKey("CbjvJc1SNx1aav8tU49dJGHu8EUdzQJSMtkjDmV8miqK"),
      accountCompressionAuthority: new web3.PublicKey("GBP8yM8xj7Ls1rMaFTLLHm1MHf1WL1f5NKdqXYRLKaGz"),
      registeredProgramPda: new web3.PublicKey("Gvnx929TW5SNWHCNc1N1F23fTJ2dELG9Y8D39fmWWGVb"),
      noopProgram: new web3.PublicKey("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV"),
      solPoolPda: web3.PublicKey.default,
    };
    return new SystemAccountPubkeys(
      defaults.lightSystemProgram,
      defaults.systemProgram,
      defaults.accountCompressionProgram,
      defaults.accountCompressionAuthority,
      defaults.registeredProgramPda,
      defaults.noopProgram,
      defaults.solPoolPda,
    );
  }
}
