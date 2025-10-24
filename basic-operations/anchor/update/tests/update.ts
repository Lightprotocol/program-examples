import * as anchor from "@coral-xyz/anchor";
import { Program, web3 } from "@coral-xyz/anchor";
import { Update } from "../target/types/update";
import updateIdl from "../target/idl/update.json";
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
} from "@lightprotocol/stateless.js";
import * as assert from "assert";

const path = require("path");
const os = require("os");
require("dotenv").config();

const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-anchor-update", () => {
  const updateProgram = anchor.workspace.Update as Program<Update>;
  const updateCoder = new anchor.BorshCoder(updateIdl as anchor.Idl);

  it("update compressed account message", async () => {
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
    const addressTree = defaultTestStateTreeAccounts().addressTree;
    const addressQueue = defaultTestStateTreeAccounts().addressQueue;

    const messageSeed = new TextEncoder().encode("message");
    const seed = deriveAddressSeed(
      [messageSeed, signer.publicKey.toBytes()],
      new web3.PublicKey(updateProgram.idl.address),
    );
    const address = deriveAddress(seed, addressTree);

    // Step 1: Create compressed account with initial message using update program's create_account
    const createTxId = await createCompressedAccount(
      rpc,
      addressTree,
      addressQueue,
      address,
      updateProgram,
      outputStateTree,
      signer,
      "Hello, compressed world!",
    );
    console.log("Create Transaction ID:", createTxId);

    // Wait for indexer to process the create transaction
    let slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    // Step 2: Get the created account
    let compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    let myAccount = updateCoder.types.decode(
      "MyCompressedAccount",
      compressedAccount.data.data,
    );
    assert.strictEqual(myAccount.message, "Hello, compressed world!");
    assert.ok(myAccount.owner.equals(signer.publicKey), "Owner should match signer public key");
    console.log("Created message:", myAccount.message);

    // Step 3: Update the account with new message
    const updateTxId = await updateCompressedAccount(
      rpc,
      compressedAccount,
      updateProgram,
      outputStateTree,
      signer,
      "Hello again, compressed World!",
    );
    console.log("Update Transaction ID:", updateTxId);

    // Wait for indexer to process the update transaction
    slot = await rpc.getSlot();
    await rpc.confirmTransactionIndexed(slot);

    // Step 4: Verify the update
    compressedAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    myAccount = updateCoder.types.decode(
      "MyCompressedAccount",
      compressedAccount.data.data,
    );
    console.log("Updated message:", myAccount.message);

    assert.ok(myAccount.owner.equals(signer.publicKey), "Owner should match signer public key");
    assert.strictEqual(myAccount.message, "Hello again, compressed World!", "Message should be updated");
  });
});

async function createCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  addressQueue: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Update>,
  outputStateTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
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
  const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
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
  await rpc.confirmTransaction(sig);
  return sig;
}

async function updateCompressedAccount(
  rpc: Rpc,
  compressedAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Update>,
  outputStateTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
  newMessage: string,
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

  const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
  let remainingAccounts =
    PackedAccounts.newWithSystemAccounts(systemAccountConfig);

  const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.tree,
  );
  const queuePubkeyIndex = remainingAccounts.insertOrGet(
    compressedAccount.treeInfo.queue,
  );
  const outputStateTreeIndex =
    remainingAccounts.insertOrGet(outputStateTree);

  // Deserialize current account using update program's coder
  const coder = new anchor.BorshCoder(updateIdl as anchor.Idl);
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
    outputStateTreeIndex,
    address: compressedAccount.address,
  };

  let proof = {
    0: proofRpcResult.compressedProof,
  };
  const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
    units: 1000000,
  });
  let tx = await program.methods
    .updateAccount(proof, currentAccount, compressedAccountMeta, newMessage)
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
  return sig;
}


class PackedAccounts {
  private preAccounts: web3.AccountMeta[] = [];
  private systemAccounts: web3.AccountMeta[] = [];
  private nextIndex: number = 0;
  private map: Map<web3.PublicKey, [number, web3.AccountMeta]> = new Map();

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
    return new SystemAccountPubkeys(
      LightSystemProgram.programId,
      web3.PublicKey.default,
      defaultStaticAccountsStruct().accountCompressionProgram,
      defaultStaticAccountsStruct().accountCompressionAuthority,
      defaultStaticAccountsStruct().registeredProgramPda,
      defaultStaticAccountsStruct().noopProgram,
      web3.PublicKey.default,
    );
  }
}
