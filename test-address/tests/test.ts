import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { TestAddress } from "../target/types/test_address";
import idl from "../target/idl/test_address.json";
import {
  bn,
  createRpc,
  defaultTestStateTreeAccounts,
  deriveAddress,
  deriveAddressSeed,
  Rpc,
  sleep,
  AddressWithTree,
  getDefaultAddressTreeInfo,
} from "@lightprotocol/stateless.js";
import { keccak_256 } from "@noble/hashes/sha3";

const path = require("path");
const os = require("os");
require("dotenv").config();

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-address", () => {
  const program = anchor.workspace.TestAddress as Program<TestAddress>;

  it("claim with address derivation", async () => {
    let payer = new web3.Keypair();
    let rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      {
        commitment: "confirmed",
      }
    );

    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(payer.publicKey, lamports);
    await sleep(2000);

    // Generate a random submission ID
    const submissionId = new Uint8Array(32);
    crypto.getRandomValues(submissionId);

    // Get the submission seed constant from IDL

    console.log("program.idl.constants", program.idl.constants);
    console.log(
      "program.idl.constants[0].value",
      program.idl.constants[0].value
    );
    console.log(
      "Array.from(Buffer.from(program.idl.constants[0].value))",
      Array.from(Buffer.from(program.idl.constants[0].value))
    );

    console.log("submissionId", Array.from(submissionId));

    const conn = rpc;
    const params = getDefaultAddressTreeInfo();

    const submissionSeed = new Uint8Array(
      JSON.parse(program.idl.constants[0].value)
    );
    console.log("submissionSeed", submissionSeed);

    const newAddress = deriveAddress(
      deriveAddressSeed(
        [Buffer.from(JSON.parse(program.idl.constants[0].value)), submissionId],
        program.programId
      ),
      params.tree
    );

    console.log("newAddress", newAddress.toBytes());

    const newAddressWithTree: AddressWithTree = {
      address: bn(newAddress.toBuffer()),
      tree: params.tree,
      queue: params.queue,
    };

    console.log("newAddressWithTree", newAddressWithTree);

    const proofResult = await conn.getValidityProofV0(undefined, [
      newAddressWithTree,
    ]);
    const proof = proofResult.compressedProof;
    console.log("Proof result:", proofResult);

    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    const addressMerkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      params.tree
    );
    const addressQueuePubkeyIndex = remainingAccounts.insertOrGet(params.queue);

    const packedAddreesMerkleContext = {
      rootIndex: proofResult.rootIndices[0],
      addressMerkleTreePubkeyIndex,
      addressQueuePubkeyIndex,
    };

    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });

    const ix = await program.methods
      .claim(Array.from(submissionId), { 0: proof }, packedAddreesMerkleContext)
      .accounts({
        feePayer: payer.publicKey,
      })
      .preInstructions([computeBudgetIx])
      .remainingAccounts(remainingAccounts.toAccountMetas().remainingAccounts)
      .instruction();

    const tx = new web3.Transaction().add(ix);
    tx.recentBlockhash = (await rpc.getRecentBlockhash()).blockhash;
    tx.sign(payer);

    const sig = await rpc.sendTransaction(tx, [payer]);
    await rpc.confirmTransaction(sig);
    console.log("Claim transaction signature:", sig);
  });
});

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

function getLightSystemAccountMetas(
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
    solPoolPda: web3.PublicKey
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
    const {
      LightSystemProgram,
      defaultStaticAccountsStruct,
    } = require("@lightprotocol/stateless.js");
    return new SystemAccountPubkeys(
      LightSystemProgram.programId,
      web3.PublicKey.default,
      defaultStaticAccountsStruct().accountCompressionProgram,
      defaultStaticAccountsStruct().accountCompressionAuthority,
      defaultStaticAccountsStruct().registeredProgramPda,
      defaultStaticAccountsStruct().noopProgram,
      web3.PublicKey.default
    );
  }
}
