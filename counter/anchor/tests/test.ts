/**
 * Light Protocol Compressed Account Test Suite
 *
 * This test suite demonstrates how to work with compressed accounts using the Light Protocol.
 * Compressed accounts provide state compression on Solana, reducing storage costs while
 * maintaining security through Merkle tree proofs.
 */

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
} from "@lightprotocol/stateless.js";

const path = require("path");
const os = require("os");
require("dotenv").config();

// Set up the default Solana wallet path for Anchor
const anchorWalletPath = path.join(os.homedir(), ".config/solana/id.json");
process.env.ANCHOR_WALLET = anchorWalletPath;

describe("test-anchor", () => {
  // Initialize the Counter program from Anchor workspace
  const program = anchor.workspace.Counter as Program<Counter>;
  // Create a Borsh coder for serializing/deserializing account data
  const coder = new anchor.BorshCoder(idl as anchor.Idl);

  it("", async () => {
    // Generate a new keypair to use as transaction signer
    let signer = new web3.Keypair();

    // Create RPC connection to local Solana validator and Light Protocol services
    // - Port 8899: Solana RPC
    // - Port 8784: Light Protocol Prover
    // - Port 3001: Light Protocol Indexer
    let rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      {
        commitment: "confirmed",
      },
    );

    // Fund the signer account with 1 SOL for transaction fees
    let lamports = web3.LAMPORTS_PER_SOL;
    await rpc.requestAirdrop(signer.publicKey, lamports);
    await sleep(2000);

    // Get default test Merkle tree accounts for compressed state storage
    const outputMerkleTree = defaultTestStateTreeAccounts().merkleTree;
    const addressTree = defaultTestStateTreeAccounts().addressTree;
    const addressQueue = defaultTestStateTreeAccounts().addressQueue;

    // Derive a deterministic address for the compressed counter account
    // Uses "counter" seed + signer's public key for uniqueness
    const counterSeed = new TextEncoder().encode("counter");
    const seed = deriveAddressSeed(
      [counterSeed, signer.publicKey.toBytes()],
      new web3.PublicKey(program.idl.address),
    );
    const address = deriveAddress(seed, addressTree);

    // Create counter compressed account.
    await CreateCounterCompressedAccount(
      rpc,
      addressTree,
      addressQueue,
      address,
      program,
      outputMerkleTree,
      signer,
    );
    // Wait for indexer to catch up.
    await sleep(2000);

    // Fetch the created compressed account and decode its data
    let counterAccount = await rpc.getCompressedAccount(bn(address.toBytes()));

    let counter = coder.types.decode(
      "CounterAccount",
      counterAccount.data.data,
    );
    console.log("counter account ", counterAccount);
    console.log("des counter ", counter);

    // Increment the counter value in the compressed account
    await incrementCounterCompressedAccount(
      rpc,
      counter.value,
      counterAccount,
      program,
      outputMerkleTree,
      signer,
    );

    // Wait for indexer to catch up.
    await sleep(2000);

    // Fetch and decode the updated counter account
    counterAccount = await rpc.getCompressedAccount(bn(address.toBytes()));
    counter = coder.types.decode("CounterAccount", counterAccount.data.data);
    console.log("counter account ", counterAccount);
    console.log("des counter ", counter);

    // Delete the compressed counter account
    await deleteCounterCompressedAccount(
      rpc,
      counter.value,
      counterAccount,
      program,
      outputMerkleTree,
      signer,
    );

    // Wait for indexer to catch up.
    await sleep(2000);

    // Verify the account has been deleted
    const deletedCounterAccount = await rpc.getCompressedAccount(
      bn(address.toBytes()),
    );
    console.log("deletedCounterAccount ", deletedCounterAccount);
  });
});

/**
 * Creates a new compressed counter account on-chain
 *
 * @param rpc - Light Protocol RPC client
 * @param addressTree - Merkle tree storing compressed account addresses
 * @param addressQueue - Queue for processing address tree updates
 * @param address - Derived address for the counter account
 * @param program - Anchor program instance for the Counter contract
 * @param outputMerkleTree - Merkle tree where new compressed state will be stored
 * @param signer - Keypair that will sign and pay for the transaction
 */
async function CreateCounterCompressedAccount(
  rpc: Rpc,
  addressTree: anchor.web3.PublicKey,
  addressQueue: anchor.web3.PublicKey,
  address: anchor.web3.PublicKey,
  program: anchor.Program<Counter>,
  outputMerkleTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
) {
  {
    // Generate a validity proof for the address derivation
    // This proves the address doesn't already exist in the address tree
    const proofRpcResult = await rpc.getValidityProofV0(
      [], // No existing compressed accounts to prove
      [
        {
          tree: addressTree,
          queue: addressQueue,
          address: bn(address.toBytes()),
        },
      ],
    );

    // Configure system accounts required for Light Protocol operations
    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    // Add Merkle tree accounts to the remaining accounts list and get their indices
    const addressMerkleTreePubkeyIndex =
      remainingAccounts.insertOrGet(addressTree);
    const addressQueuePubkeyIndex = remainingAccounts.insertOrGet(addressQueue);

    // Package the address Merkle context for the instruction
    const packedAddreesMerkleContext = {
      rootIndex: proofRpcResult.rootIndices[0],
      addressMerkleTreePubkeyIndex,
      addressQueuePubkeyIndex,
    };
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(outputMerkleTree);

    // Format the compressed proof for the instruction
    let proof = {
      0: proofRpcResult.compressedProof,
    };

    // Set compute budget to handle complex compressed account operations
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });

    // Build and send the create counter transaction
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

/**
 * Increments the value in an existing compressed counter account
 *
 * This demonstrates the update pattern for compressed accounts:
 * 1. Prove the current state exists
 * 2. Create a new state with updated data
 * 3. The old state is automatically nullified
 *
 * @param rpc - Light Protocol RPC client
 * @param counterValue - Current value of the counter
 * @param counterAccount - The compressed account data with Merkle context
 * @param program - Anchor program instance for the Counter contract
 * @param outputMerkleTree - Merkle tree where updated state will be stored
 * @param signer - Keypair that will sign and pay for the transaction
 */
async function incrementCounterCompressedAccount(
  rpc: Rpc,
  counterValue: anchor.BN,
  counterAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Counter>,
  outputMerkleTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
) {
  {
    // Generate a validity proof for the existing compressed account
    // This proves the account exists and we know its current state
    const proofRpcResult = await rpc.getValidityProofV0(
      [
        {
          hash: counterAccount.hash,
          tree: counterAccount.treeInfo.tree,
          queue: counterAccount.treeInfo.queue,
        },
      ],
      [], // No new addresses being created
    );

    // Configure system accounts for Light Protocol operations
    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    // Add required Merkle tree accounts and get their indices
    const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.tree,
    );
    const queuePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.queue,
    );
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(outputMerkleTree);

    // Package the compressed account metadata for the instruction
    const compressedAccountMeta = {
      treeInfo: {
        rootIndex: proofRpcResult.rootIndices[0],
        proveByIndex: false, // Prove by hash, not by leaf index
        merkleTreePubkeyIndex,
        queuePubkeyIndex,
        leafIndex: counterAccount.leafIndex,
      },
      address: counterAccount.address,
      outputStateTreeIndex: outputMerkleTreeIndex,
    };

    // Format the compressed proof for the instruction
    let proof = {
      0: proofRpcResult.compressedProof,
    };

    // Set compute budget for compressed account operations
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });

    // Build and send the increment counter transaction
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

/**
 * Deletes a compressed counter account by nullifying it in the Merkle tree
 *
 * Unlike regular Solana accounts, compressed accounts are "deleted" by proving
 * their existence and then nullifying them without creating new state.
 *
 * @param rpc - Light Protocol RPC client
 * @param counterValue - Current value of the counter (for validation)
 * @param counterAccount - The compressed account data with Merkle context
 * @param program - Anchor program instance for the Counter contract
 * @param outputMerkleTree - Merkle tree (unused in deletion, but kept for consistency)
 * @param signer - Keypair that will sign and pay for the transaction
 */
async function deleteCounterCompressedAccount(
  rpc: Rpc,
  counterValue: anchor.BN,
  counterAccount: CompressedAccountWithMerkleContext,
  program: anchor.Program<Counter>,
  outputMerkleTree: anchor.web3.PublicKey,
  signer: anchor.web3.Keypair,
) {
  {
    // Generate validity proof for the account to be deleted
    const proofRpcResult = await rpc.getValidityProofV0(
      [
        {
          hash: counterAccount.hash,
          tree: counterAccount.treeInfo.tree,
          queue: counterAccount.treeInfo.queue,
        },
      ],
      [], // No new addresses being created
    );

    // Configure system accounts for Light Protocol operations
    const systemAccountConfig = SystemAccountMetaConfig.new(program.programId);
    let remainingAccounts =
      PackedAccounts.newWithSystemAccounts(systemAccountConfig);

    // Add required Merkle tree accounts and get their indices
    const merkleTreePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.tree,
    );
    const queuePubkeyIndex = remainingAccounts.insertOrGet(
      counterAccount.treeInfo.queue,
    );
    const outputMerkleTreeIndex =
      remainingAccounts.insertOrGet(outputMerkleTree);

    // Package the compressed account metadata for deletion
    // Note: No outputStateTreeIndex since we're not creating new state
    const compressedAccountMeta = {
      treeInfo: {
        rootIndex: proofRpcResult.rootIndices[0],
        proveByIndex: false,
        merkleTreePubkeyIndex,
        queuePubkeyIndex,
        leafIndex: counterAccount.leafIndex,
      },
      address: counterAccount.address,
    };

    // Format the compressed proof for the instruction
    let proof = {
      0: proofRpcResult.compressedProof,
    };

    // Set compute budget for compressed account operations
    const computeBudgetIx = web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 1000000,
    });

    // Build and send the close counter transaction
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

/**
 * PackedAccounts manages the complex account structure required for Light Protocol transactions.
 *
 * Solana transactions have a limit on the number of accounts they can reference directly.
 * PackedAccounts helps organize accounts into categories and provides indices for referencing
 * them in instruction data, allowing for more complex operations.
 */
class PackedAccounts {
  /** Accounts that must be included before system accounts (typically signers) */
  private preAccounts: web3.AccountMeta[] = [];
  /** Standard Light Protocol system accounts required for compressed operations */
  private systemAccounts: web3.AccountMeta[] = [];
  /** Counter for assigning unique indices to dynamically added accounts */
  private nextIndex: number = 0;
  /** Map to deduplicate accounts and track their assigned indices */
  private map: Map<web3.PublicKey, [number, web3.AccountMeta]> = new Map();

  /**
   * Creates a new PackedAccounts instance with Light Protocol system accounts pre-configured
   */
  static newWithSystemAccounts(
    config: SystemAccountMetaConfig,
  ): PackedAccounts {
    const instance = new PackedAccounts();
    instance.addSystemAccounts(config);
    return instance;
  }

  /** Adds a signer account to the pre-accounts list (read-only) */
  addPreAccountsSigner(pubkey: web3.PublicKey): void {
    this.preAccounts.push({ pubkey, isSigner: true, isWritable: false });
  }

  /** Adds a signer account to the pre-accounts list (writable) */
  addPreAccountsSignerMut(pubkey: web3.PublicKey): void {
    this.preAccounts.push({ pubkey, isSigner: true, isWritable: true });
  }

  /** Adds a custom account meta to the pre-accounts list */
  addPreAccountsMeta(accountMeta: web3.AccountMeta): void {
    this.preAccounts.push(accountMeta);
  }

  /** Adds all required Light Protocol system accounts */
  addSystemAccounts(config: SystemAccountMetaConfig): void {
    this.systemAccounts.push(...getLightSystemAccountMetas(config));
  }

  /**
   * Inserts an account or returns its existing index (writable by default)
   * This is the most common method for adding Merkle tree accounts
   */
  insertOrGet(pubkey: web3.PublicKey): number {
    return this.insertOrGetConfig(pubkey, false, true);
  }

  /** Inserts an account or returns its existing index (read-only) */
  insertOrGetReadOnly(pubkey: web3.PublicKey): number {
    return this.insertOrGetConfig(pubkey, false, false);
  }

  /**
   * Core method for inserting accounts with custom signer/writable configuration
   * Returns the index that can be used to reference this account in instruction data
   */
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

  /** Converts the internal map to a sorted array of AccountMetas */
  private hashSetAccountsToMetas(): web3.AccountMeta[] {
    const entries = Array.from(this.map.entries());
    entries.sort((a, b) => a[1][0] - b[1][0]);
    return entries.map(([, [, meta]]) => meta);
  }

  /** Calculates the starting indices for different account categories */
  private getOffsets(): [number, number] {
    const systemStart = this.preAccounts.length;
    const packedStart = systemStart + this.systemAccounts.length;
    return [systemStart, packedStart];
  }

  /**
   * Generates the final account list for transaction construction
   * Returns the accounts in the correct order: pre-accounts, system accounts, then dynamic accounts
   */
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

/**
 * Configuration for Light Protocol system accounts required in compressed account operations.
 *
 * Different operations may require different combinations of system accounts.
 * This class provides a flexible way to configure which accounts are needed.
 */
class SystemAccountMetaConfig {
  /** The program making CPI calls to Light Protocol */
  selfProgram: web3.PublicKey;
  /** Optional: Account for storing CPI context data */
  cpiContext?: web3.PublicKey;
  /** Optional: Recipient account for SOL compression operations */
  solCompressionRecipient?: web3.PublicKey;
  /** Optional: PDA for managing SOL pool operations */
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

  /** Creates a basic configuration with just the calling program */
  static new(selfProgram: web3.PublicKey): SystemAccountMetaConfig {
    return new SystemAccountMetaConfig(selfProgram);
  }

  /** Creates a configuration with CPI context support */
  static newWithCpiContext(
    selfProgram: web3.PublicKey,
    cpiContext: web3.PublicKey,
  ): SystemAccountMetaConfig {
    return new SystemAccountMetaConfig(selfProgram, cpiContext);
  }
}

/**
 * Generates the standard set of Light Protocol system accounts required for compressed operations.
 *
 * These accounts include:
 * - Light System Program: Core compressed account logic
 * - CPI Signer: PDA that signs on behalf of the calling program
 * - Account Compression Program: Handles Merkle tree operations
 * - Various other system accounts for protocol functionality
 *
 * @param config - Configuration specifying which optional accounts to include
 * @returns Array of AccountMeta objects for all required system accounts
 */
function getLightSystemAccountMetas(
  config: SystemAccountMetaConfig,
): web3.AccountMeta[] {
  // Derive the CPI authority PDA that will sign Light Protocol transactions
  let signerSeed = new TextEncoder().encode("cpi_authority");
  const cpiSigner = web3.PublicKey.findProgramAddressSync(
    [signerSeed],
    config.selfProgram,
  )[0];

  // Get default system account addresses
  const defaults = SystemAccountPubkeys.default();

  // Build the core set of required accounts
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

  // Add optional accounts if configured
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

  // System program is always required
  metas.push({
    pubkey: defaults.systemProgram,
    isSigner: false,
    isWritable: false,
  });

  // CPI context is optional and added last if present
  if (config.cpiContext) {
    metas.push({
      pubkey: config.cpiContext,
      isSigner: false,
      isWritable: true,
    });
  }
  return metas;
}

/**
 * Contains the public keys for all Light Protocol system accounts.
 *
 * These are the core accounts that make up the Light Protocol infrastructure
 * on Solana. Most of these addresses are deterministic and don't change between
 * deployments, but this class provides a clean way to access them.
 */
class SystemAccountPubkeys {
  /** The main Light Protocol system program */
  lightSystemProgram: web3.PublicKey;
  /** Solana's native system program */
  systemProgram: web3.PublicKey;
  /** Program that handles Merkle tree compression operations */
  accountCompressionProgram: web3.PublicKey;
  /** Authority account for the compression program */
  accountCompressionAuthority: web3.PublicKey;
  /** PDA that tracks registered programs authorized to use Light Protocol */
  registeredProgramPda: web3.PublicKey;
  /** No-op program for logging/event emission */
  noopProgram: web3.PublicKey;
  /** PDA for managing SOL compression pools */
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

  /**
   * Returns the standard set of Light Protocol system account addresses
   * These addresses are typically the same across different environments
   */
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
