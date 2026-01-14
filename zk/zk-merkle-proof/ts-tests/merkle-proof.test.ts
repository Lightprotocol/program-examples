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
import { keccak_256 } from "@noble/hashes/sha3";
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
  fs.readFileSync(path.join(process.cwd(), "target/idl/zk_merkle_proof.json"), "utf8")
);

// Program ID from IDL
const PROGRAM_ID = new web3.PublicKey(IDL.address);
const ZK_ACCOUNT_PREFIX = Buffer.from("zk_account");
const ZK_ACCOUNT_DISCRIMINATOR = Buffer.from([0x5b, 0x98, 0xb8, 0x43, 0x93, 0x6c, 0x21, 0xf4]);

// Paths to circuit artifacts
const BUILD_DIR = path.join(process.cwd(), "build");
const WASM_PATH = path.join(BUILD_DIR, "merkle_proof_js/merkle_proof.wasm");
const ZKEY_PATH = path.join(BUILD_DIR, "merkle_proof_final.zkey");

const MERKLE_TREE_DEPTH = 26;

/** Hash to BN254 field (matching Light Protocol's hashv_to_bn254_field_size_be) */
function hashToBn254Field(data: Uint8Array): Uint8Array {
  const hash = keccak_256(data);
  hash[0] = hash[0] & 0x1f;
  return hash;
}

describe("zk-merkle-proof", () => {
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

  /** Compute Poseidon hash of compressed account fields */
  function computeAccountHash(
    ownerHashed: Uint8Array,
    leafIndex: bigint,
    merkleTreeHashed: Uint8Array,
    address: Uint8Array,
    discriminator: bigint,
    dataHash: Uint8Array
  ): Uint8Array {
    const LAMPORTS_OFFSET = 36893488147419103232n;
    const hash = poseidon([
      BigInt("0x" + Buffer.from(ownerHashed).toString("hex")),
      leafIndex,
      BigInt("0x" + Buffer.from(merkleTreeHashed).toString("hex")),
      BigInt("0x" + Buffer.from(address).toString("hex")),
      discriminator + LAMPORTS_OFFSET,
      BigInt("0x" + Buffer.from(dataHash).toString("hex")),
    ]);
    return bigintToBytes32(poseidon.F.toObject(hash));
  }

  /** Compute Merkle root from leaf and path */
  function computeMerkleRoot(leaf: Uint8Array, pathElements: Uint8Array[], leafIndex: number): Uint8Array {
    let current = BigInt("0x" + Buffer.from(leaf).toString("hex"));

    for (let i = 0; i < pathElements.length; i++) {
      const pathElement = BigInt("0x" + Buffer.from(pathElements[i]).toString("hex"));
      const isRight = (leafIndex >> i) & 1;
      const [left, right] = isRight ? [pathElement, current] : [current, pathElement];
      current = poseidon.F.toObject(poseidon([left, right]));
    }

    return bigintToBytes32(current);
  }

  /** Generate ZK proof for Merkle inclusion */
  async function generateMerkleProof(
    ownerHashed: Uint8Array,
    merkleTreeHashed: Uint8Array,
    discriminator: Uint8Array,
    dataHash: Uint8Array,
    expectedRoot: Uint8Array,
    leafIndex: number,
    accountLeafIndex: number,
    address: Uint8Array,
    pathElements: Uint8Array[]
  ): Promise<{ a: number[]; b: number[]; c: number[] }> {
    const inputs = {
      owner_hashed: toFieldString(ownerHashed),
      merkle_tree_hashed: toFieldString(merkleTreeHashed),
      discriminator: toFieldString(discriminator),
      data_hash: toFieldString(dataHash),
      expectedRoot: toFieldString(expectedRoot),
      leaf_index: leafIndex.toString(),
      account_leaf_index: accountLeafIndex.toString(),
      address: toFieldString(address),
      pathElements: pathElements.map(toFieldString),
    };

    const { proof } = await snarkjs.groth16.fullProve(inputs, WASM_PATH, ZKEY_PATH);
    return parseProofToCompressed(proof);
  }

  /** Build create_account instruction using Anchor */
  async function buildCreateAccountInstruction(dataHash: Uint8Array): Promise<web3.TransactionInstruction> {
    const addressTree = new web3.PublicKey(batchAddressTree);
    const outputStateTree = defaultTestStateTreeAccounts().merkleTree;

    const seed = deriveAddressSeedV2([ZK_ACCOUNT_PREFIX, dataHash]);
    const address = deriveAddressV2(seed, addressTree, PROGRAM_ID);

    const proofResult = await rpc.getValidityProofV0(
      [],
      [{ tree: addressTree, queue: addressTree, address: bn(address.toBytes()) }]
    );

    const remainingAccounts = new PackedAccounts();
    remainingAccounts.addPreAccountsSigner(signer.publicKey);
    remainingAccounts.addSystemAccountsV2(SystemAccountMetaConfig.new(PROGRAM_ID));

    const addressMerkleTreeIndex = remainingAccounts.insertOrGet(addressTree);
    const outputStateTreeIndex = remainingAccounts.insertOrGet(outputStateTree);

    const { remainingAccounts: accountMetas, systemStart } = remainingAccounts.toAccountMetas();

    // Use Anchor to build instruction
    // ValidityProof is a struct with an unnamed Option<CompressedProof> field
    const proof = {
      0: proofResult.compressedProof,
    };

    const ix = await program.methods
      .createAccount(
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
        // data_hash
        Array.from(dataHash)
      )
      .accounts({
        signer: signer.publicKey,
      })
      .remainingAccounts(accountMetas)
      .instruction();

    return ix;
  }

  describe("create_account", () => {
    it("should create a compressed account with data hash", async () => {
      const dataHash = generateFieldElement();
      console.log("Data hash:", Buffer.from(dataHash).toString("hex").slice(0, 16) + "...");

      const ix = await buildCreateAccountInstruction(dataHash);
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
      assert.ok(accounts.items.length > 0, "Account should be created");
      console.log("Created accounts:", accounts.items.length);
    });
  });

  describe("verify_account (ZK proof)", () => {
    it("should verify account existence with ZK proof", async () => {
      const dataHash = generateFieldElement();

      const createIx = await buildCreateAccountInstruction(dataHash);
      const computeIx = web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 });

      const createTx = new web3.Transaction().add(computeIx, createIx);
      createTx.recentBlockhash = (await rpc.getLatestBlockhash()).blockhash;
      createTx.feePayer = signer.publicKey;
      createTx.sign(signer);

      await rpc.sendTransaction(createTx, [signer]);
      await sleep(3000);

      const slot = await rpc.getSlot();
      await rpc.confirmTransactionIndexed(slot);

      const accounts = await rpc.getCompressedAccountsByOwner(PROGRAM_ID);
      assert.ok(accounts.items.length > 0, "Should have created account");

      const account = accounts.items[0];
      console.log("Account hash:", account.hash.toString(16).slice(0, 16) + "...");
      console.log("Leaf index:", account.leafIndex);

      const merkleProof = await rpc.getValidityProof([account.hash]);
      console.log("Root index:", merkleProof.rootIndices[0]);

      assert.ok(merkleProof.compressedProof, "Should have compressed proof");
      assert.ok(merkleProof.rootIndices.length > 0, "Should have root indices");

      console.log("Account verified in state tree");
    });

    it("should demonstrate ZK proof generation for Merkle inclusion", async () => {
      const ownerHashed = hashToBn254Field(PROGRAM_ID.toBytes());
      const merkleTreeHashed = hashToBn254Field(
        new web3.PublicKey(defaultTestStateTreeAccounts().merkleTree).toBytes()
      );

      const dataHash = generateFieldElement();
      const discriminator = new Uint8Array(32);
      discriminator.set(ZK_ACCOUNT_DISCRIMINATOR, 24);

      const pathElements = Array.from({ length: MERKLE_TREE_DEPTH }, () => new Uint8Array(32));
      const address = generateFieldElement();

      const accountHash = computeAccountHash(
        ownerHashed,
        0n,
        merkleTreeHashed,
        address,
        BigInt("0x" + Buffer.from(discriminator).toString("hex")),
        dataHash
      );

      const expectedRoot = computeMerkleRoot(accountHash, pathElements, 0);

      console.log("Account hash:", Buffer.from(accountHash).toString("hex").slice(0, 16) + "...");
      console.log("Expected root:", Buffer.from(expectedRoot).toString("hex").slice(0, 16) + "...");
      console.log("Generating ZK proof...");

      const zkProof = await generateMerkleProof(
        ownerHashed,
        merkleTreeHashed,
        discriminator,
        dataHash,
        expectedRoot,
        0,
        0,
        address,
        pathElements
      );

      assert.ok(zkProof.a.length === 32, "Proof A should be 32 bytes");
      assert.ok(zkProof.b.length === 64, "Proof B should be 64 bytes");
      assert.ok(zkProof.c.length === 32, "Proof C should be 32 bytes");

      console.log("ZK Merkle proof generated successfully");
    });
  });
});
