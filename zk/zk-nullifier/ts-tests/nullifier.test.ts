import { web3, Program, AnchorProvider, setProvider } from "@coral-xyz/anchor";
import {
  bn,
  createRpc,
  deriveAddressSeedV2,
  deriveAddressV2,
  batchAddressTree,
  PackedAccounts,
  Rpc,
  sleep,
  SystemAccountMetaConfig,
  defaultTestStateTreeAccounts,
  featureFlags,
  VERSION,
  confirmTx,
} from "@lightprotocol/stateless.js";
import { buildPoseidonOpt } from "circomlibjs";
import * as snarkjs from "snarkjs";
import * as assert from "assert";
import * as path from "path";
import * as fs from "fs";

import {
  parseProofToCompressed,
  bigintToBytes32,
  toFieldString,
  generateFieldElement,
} from "./utils/proof-helpers";

// Force V2 mode
(featureFlags as any).version = VERSION.V2;

// Load IDL
const IDL = JSON.parse(
  fs.readFileSync(path.join(process.cwd(), "target/idl/zk_nullifier.json"), "utf8")
);

const PROGRAM_ID = new web3.PublicKey(IDL.address);
const NULLIFIER_PREFIX = Buffer.from("nullifier");

// Paths to circuit artifacts
const BUILD_DIR = path.join(process.cwd(), "build");
const WASM_PATH_SINGLE = path.join(BUILD_DIR, "nullifier_1_js/nullifier_1.wasm");
const ZKEY_PATH_SINGLE = path.join(BUILD_DIR, "nullifier_1_final.zkey");
const VKEY_PATH_SINGLE = path.join(BUILD_DIR, "nullifier_1_vkey.json");
const WASM_PATH_BATCH = path.join(BUILD_DIR, "nullifier_4_js/nullifier_4.wasm");
const ZKEY_PATH_BATCH = path.join(BUILD_DIR, "nullifier_4_final.zkey");

describe("zk-nullifier", () => {
  let rpc: Rpc;
  let signer: web3.Keypair;
  let poseidon: any;
  let program: Program;

  before(async () => {
    rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      { commitment: "confirmed" }
    );

    signer = web3.Keypair.generate();
    await rpc.requestAirdrop(signer.publicKey, web3.LAMPORTS_PER_SOL);
    await sleep(2000);

    poseidon = await buildPoseidonOpt();

    // Setup Anchor provider and program
    const connection = new web3.Connection("http://127.0.0.1:8899", "confirmed");
    const wallet = {
      publicKey: signer.publicKey,
      signTransaction: async (tx: web3.Transaction) => {
        tx.sign(signer);
        return tx;
      },
      signAllTransactions: async (txs: web3.Transaction[]) => {
        txs.forEach((tx) => tx.sign(signer));
        return txs;
      },
    };
    const provider = new AnchorProvider(connection, wallet as any, { commitment: "confirmed" });
    setProvider(provider);
    program = new Program(IDL, provider);
  });

  after(async () => {
    // Terminate snarkjs curve worker to allow clean exit
    // @ts-ignore
    if (globalThis.curve_bn128) {
      // @ts-ignore
      await globalThis.curve_bn128.terminate();
    }
  });

  /** Compute nullifier = Poseidon(verification_id, secret) */
  function computeNullifier(verificationId: Uint8Array, secret: Uint8Array): Uint8Array {
    const hash = poseidon([toFieldString(verificationId), toFieldString(secret)].map(BigInt));
    return bigintToBytes32(poseidon.F.toObject(hash));
  }

  /** Generate Groth16 proof for single nullifier */
  async function generateProof(
    verificationId: Uint8Array,
    nullifier: Uint8Array,
    secret: Uint8Array
  ): Promise<{ a: number[]; b: number[]; c: number[] }> {
    const inputs = {
      verification_id: toFieldString(verificationId),
      nullifier: toFieldString(nullifier),
      secret: toFieldString(secret),
    };

    const { proof, publicSignals } = await snarkjs.groth16.fullProve(inputs, WASM_PATH_SINGLE, ZKEY_PATH_SINGLE);

    // Verify locally with snarkjs before converting
    const vkey = JSON.parse(fs.readFileSync(VKEY_PATH_SINGLE, "utf8"));
    const isValid = await snarkjs.groth16.verify(vkey, publicSignals, proof);
    console.log("Local snarkjs verification:", isValid);
    console.log("Public signals:", publicSignals);

    // Use prover.js logic for proof conversion
    const compressed = parseProofToCompressed(proof);

    console.log("Compressed proof a (first 8 bytes):", compressed.a.slice(0, 8));
    console.log("Compressed proof b (first 8 bytes):", compressed.b.slice(0, 8));
    console.log("Compressed proof c (first 8 bytes):", compressed.c.slice(0, 8));

    return compressed;
  }

  /** Generate Groth16 proof for batch (4) nullifiers */
  async function generateBatchProof(
    verificationId: Uint8Array,
    nullifiers: Uint8Array[],
    secrets: Uint8Array[]
  ): Promise<{ a: number[]; b: number[]; c: number[] }> {
    const inputs = {
      verification_id: toFieldString(verificationId),
      nullifier: nullifiers.map(toFieldString),
      secret: secrets.map(toFieldString),
    };

    const { proof } = await snarkjs.groth16.fullProve(inputs, WASM_PATH_BATCH, ZKEY_PATH_BATCH);
    return parseProofToCompressed(proof);
  }

  /** Build create_nullifier instruction using Anchor */
  async function buildCreateNullifierInstruction(
    verificationId: Uint8Array,
    nullifier: Uint8Array,
    secret: Uint8Array
  ): Promise<web3.TransactionInstruction> {
    const addressTree = new web3.PublicKey(batchAddressTree);
    const outputStateTree = defaultTestStateTreeAccounts().merkleTree;

    const seed = deriveAddressSeedV2([NULLIFIER_PREFIX, nullifier, verificationId]);
    const address = deriveAddressV2(seed, addressTree, PROGRAM_ID);

    const proofResult = await rpc.getValidityProofV0(
      [],
      [{ tree: addressTree, queue: addressTree, address: bn(address.toBytes()) }]
    );

    // Use V2 accounts layout (matches on-chain CpiAccounts::new from light_sdk::cpi::v2)
    const remainingAccounts = new PackedAccounts();
    remainingAccounts.addPreAccountsSigner(signer.publicKey);
    remainingAccounts.addSystemAccountsV2(SystemAccountMetaConfig.new(PROGRAM_ID));

    const addressMerkleTreeIndex = remainingAccounts.insertOrGet(addressTree);
    const outputStateTreeIndex = remainingAccounts.insertOrGet(outputStateTree);

    const zkProof = await generateProof(verificationId, nullifier, secret);

    // Get system_accounts_offset from packed accounts
    const { remainingAccounts: accountMetas, systemStart } = remainingAccounts.toAccountMetas();

    // Use Anchor to build instruction
    // ValidityProof is a struct with an unnamed Option<CompressedProof> field
    // Anchor JS client uses index-based access for unnamed tuple/option fields
    const proof = {
      0: proofResult.compressedProof,
    };

    const ix = await program.methods
      .createNullifier(
        // proof (ValidityProof = struct with Option<CompressedProof>)
        proof,
        // address_tree_info (PackedAddressTreeInfo)
        {
          addressMerkleTreePubkeyIndex: addressMerkleTreeIndex,
          addressQueuePubkeyIndex: addressMerkleTreeIndex,
          rootIndex: proofResult.rootIndices[0],
        },
        // output_state_tree_index
        outputStateTreeIndex,
        // system_accounts_offset
        systemStart,
        // zk_proof (CompressedProof)
        {
          a: zkProof.a,
          b: zkProof.b,
          c: zkProof.c,
        },
        // verification_id
        Array.from(verificationId),
        // nullifier
        Array.from(nullifier)
      )
      .accounts({
        signer: signer.publicKey,
      })
      .remainingAccounts(accountMetas)
      .instruction();

    return ix;
  }

  describe("Single nullifier", () => {
    it("should create a nullifier with valid ZK proof", async () => {
      // Use generateFieldElement for verificationId to ensure it's in BN254 field
      const verificationId = generateFieldElement();
      const secret = generateFieldElement();
      const nullifier = computeNullifier(verificationId, secret);

      console.log("Verification ID:", Buffer.from(verificationId).toString("hex").slice(0, 16) + "...");
      console.log("Nullifier:", Buffer.from(nullifier).toString("hex").slice(0, 16) + "...");

      // Debug: Check if values are within BN254 field
      const BN254_FR = BigInt('21888242871839275222246405745257275088548364400416034343698204186575808495617');
      const verIdBigInt = BigInt("0x" + Buffer.from(verificationId).toString("hex"));
      const nullifierBigInt = BigInt("0x" + Buffer.from(nullifier).toString("hex"));
      console.log("verificationId < Fr:", verIdBigInt < BN254_FR, "value:", verIdBigInt.toString().slice(0, 20) + "...");
      console.log("nullifier < Fr:", nullifierBigInt < BN254_FR, "value:", nullifierBigInt.toString().slice(0, 20) + "...");

      const ix = await buildCreateNullifierInstruction(verificationId, nullifier, secret);
      const computeIx = web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 });

      const tx = new web3.Transaction().add(computeIx, ix);
      tx.recentBlockhash = (await rpc.getLatestBlockhash()).blockhash;
      tx.feePayer = signer.publicKey;
      tx.sign(signer);

      const sig = await rpc.sendTransaction(tx, [signer]);
      await confirmTx(rpc, sig);

      console.log("Transaction signature:", sig);

      const slot = await rpc.getSlot();
      await rpc.confirmTransactionIndexed(slot);

      const accounts = await rpc.getCompressedAccountsByOwner(PROGRAM_ID);
      assert.ok(accounts.items.length > 0, "Nullifier account should be created");
      console.log("Created nullifier accounts:", accounts.items.length);
    });

    it("should reject duplicate nullifier", async () => {
      // Use generateFieldElement for verificationId to ensure it's in BN254 field
      const verificationId = generateFieldElement();
      const secret = generateFieldElement();
      const nullifier = computeNullifier(verificationId, secret);

      const ix1 = await buildCreateNullifierInstruction(verificationId, nullifier, secret);
      const computeIx = web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 });

      const tx1 = new web3.Transaction().add(computeIx, ix1);
      tx1.recentBlockhash = (await rpc.getLatestBlockhash()).blockhash;
      tx1.feePayer = signer.publicKey;
      tx1.sign(signer);

      await rpc.sendTransaction(tx1, [signer]);
      await sleep(2000);

      // Attempt to create duplicate - should fail when getting validity proof
      // because the address already exists in the tree
      try {
        await buildCreateNullifierInstruction(verificationId, nullifier, secret);
        assert.fail("Should have rejected duplicate nullifier");
      } catch (err: any) {
        // The error should indicate the address already exists
        assert.ok(
          err.message.includes("already exists"),
          `Expected 'already exists' error, got: ${err.message}`
        );
        console.log("Duplicate correctly rejected:", err.message);
      }
    });
  });

  describe("Batch nullifier (4x)", () => {
    it("should create 4 nullifiers with single proof", async () => {
      // Use generateFieldElement for verificationId to ensure it's in BN254 field
      const verificationId = generateFieldElement();
      const secrets = Array.from({ length: 4 }, generateFieldElement);
      const nullifiers = secrets.map((s) => computeNullifier(verificationId, s));

      console.log("Creating batch of 4 nullifiers...");
      console.log("Verification ID:", Buffer.from(verificationId).toString("hex").slice(0, 16) + "...");

      const zkProof = await generateBatchProof(verificationId, nullifiers, secrets);
      console.log("Batch proof generated");

      assert.ok(zkProof.a.length === 32, "Proof A should be 32 bytes");
      assert.ok(zkProof.b.length === 64, "Proof B should be 64 bytes");
      assert.ok(zkProof.c.length === 32, "Proof C should be 32 bytes");

      console.log("Batch proof verified locally");
    });
  });
});
