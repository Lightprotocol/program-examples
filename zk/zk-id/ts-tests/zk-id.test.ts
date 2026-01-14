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
  fs.readFileSync(path.join(process.cwd(), "target/idl/zk_id.json"), "utf8")
);

// Program ID from IDL
const PROGRAM_ID = new web3.PublicKey(IDL.address);

// Account seeds
const ISSUER_PREFIX = Buffer.from("issuer");

// Circuit paths
const BUILD_DIR = path.join(process.cwd(), "build");
const WASM_PATH = path.join(BUILD_DIR, "compressed_account_merkle_proof_js/compressed_account_merkle_proof.wasm");
const ZKEY_PATH = path.join(BUILD_DIR, "compressed_account_merkle_proof_final.zkey");

const MERKLE_TREE_DEPTH = 26;

/** Hash to BN254 field (matching Light Protocol's hashv_to_bn254_field_size_be) */
function hashToBn254Field(data: Uint8Array): Uint8Array {
  const hash = keccak_256(data);
  hash[0] = hash[0] & 0x1f;
  return hash;
}

describe("zk-id", () => {
  let rpc: Rpc;
  let issuer: web3.Keypair;
  let poseidon: any;
  let program: Program;

  before(async () => {
    rpc = createRpc(
      "http://127.0.0.1:8899",
      "http://127.0.0.1:8784",
      "http://127.0.0.1:3001",
      { commitment: "confirmed" }
    );

    issuer = web3.Keypair.generate();
    await rpc.requestAirdrop(issuer.publicKey, web3.LAMPORTS_PER_SOL * 2);
    await sleep(2000);

    poseidon = await buildPoseidonOpt();

    // Setup Anchor provider and program
    const connection = new web3.Connection("http://127.0.0.1:8899", "confirmed");
    const wallet = {
      publicKey: issuer.publicKey,
      signTransaction: async (tx: web3.Transaction) => {
        tx.sign(issuer);
        return tx;
      },
      signAllTransactions: async (txs: web3.Transaction[]) => {
        txs.forEach((tx) => tx.sign(issuer));
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

  /** Generate credential keypair: publicKey = Poseidon(privateKey) */
  function generateCredentialKeypair(): { privateKey: Uint8Array; publicKey: Uint8Array } {
    const privateKey = generateFieldElement();
    const hash = poseidon([BigInt("0x" + Buffer.from(privateKey).toString("hex"))]);
    const publicKey = bigintToBytes32(poseidon.F.toObject(hash));
    return { privateKey, publicKey };
  }

  /** Compute nullifier = Poseidon(verification_id, credentialPrivateKey) */
  function computeNullifier(verificationId: Uint8Array, credentialPrivateKey: Uint8Array): Uint8Array {
    const hash = poseidon([
      BigInt("0x" + Buffer.from(verificationId).toString("hex")),
      BigInt("0x" + Buffer.from(credentialPrivateKey).toString("hex")),
    ]);
    return bigintToBytes32(poseidon.F.toObject(hash));
  }

  /** Compute credential data hash = Poseidon(issuer_hashed, credential_pubkey) */
  function computeCredentialDataHash(issuerHashed: Uint8Array, credentialPubkey: Uint8Array): Uint8Array {
    const hash = poseidon([
      BigInt("0x" + Buffer.from(issuerHashed).toString("hex")),
      BigInt("0x" + Buffer.from(credentialPubkey).toString("hex")),
    ]);
    return bigintToBytes32(poseidon.F.toObject(hash));
  }

  /** Build create_issuer instruction using Anchor */
  async function buildCreateIssuerInstruction(): Promise<web3.TransactionInstruction> {
    const addressTree = new web3.PublicKey(batchAddressTree);
    const outputStateTree = defaultTestStateTreeAccounts().merkleTree;

    const seed = deriveAddressSeedV2([ISSUER_PREFIX, issuer.publicKey.toBytes()]);
    const address = deriveAddressV2(seed, addressTree, PROGRAM_ID);

    const proofResult = await rpc.getValidityProofV0(
      [],
      [{ tree: addressTree, queue: addressTree, address: bn(address.toBytes()) }]
    );

    const remainingAccounts = new PackedAccounts();
    remainingAccounts.addPreAccountsSigner(issuer.publicKey);
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
      .createIssuer(
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
        systemStart
      )
      .accounts({
        signer: issuer.publicKey,
      })
      .remainingAccounts(accountMetas)
      .instruction();

    return ix;
  }

  /** Generate ZK proof for credential verification */
  async function generateCredentialProof(params: {
    ownerHashed: Uint8Array;
    merkleTreeHashed: Uint8Array;
    discriminator: Uint8Array;
    issuerHashed: Uint8Array;
    expectedRoot: Uint8Array;
    verificationId: Uint8Array;
    publicEncryptedDataHash: Uint8Array;
    nullifier: Uint8Array;
    credentialPrivateKey: Uint8Array;
    leafIndex: number;
    accountLeafIndex: number;
    address: Uint8Array;
    pathElements: Uint8Array[];
    encryptedDataHash: Uint8Array;
  }): Promise<{ a: number[]; b: number[]; c: number[] }> {
    const inputs = {
      owner_hashed: toFieldString(params.ownerHashed),
      merkle_tree_hashed: toFieldString(params.merkleTreeHashed),
      discriminator: toFieldString(params.discriminator),
      issuer_hashed: toFieldString(params.issuerHashed),
      expectedRoot: toFieldString(params.expectedRoot),
      verification_id: toFieldString(params.verificationId),
      public_encrypted_data_hash: toFieldString(params.publicEncryptedDataHash),
      nullifier: toFieldString(params.nullifier),
      credentialPrivateKey: toFieldString(params.credentialPrivateKey),
      leaf_index: params.leafIndex.toString(),
      account_leaf_index: params.accountLeafIndex.toString(),
      address: toFieldString(params.address),
      pathElements: params.pathElements.map(toFieldString),
      encrypted_data_hash: toFieldString(params.encryptedDataHash),
    };

    const { proof } = await snarkjs.groth16.fullProve(inputs, WASM_PATH, ZKEY_PATH);
    return parseProofToCompressed(proof);
  }

  describe("Issuer management", () => {
    it("should create an issuer account", async () => {
      console.log("Issuer pubkey:", issuer.publicKey.toBase58());

      const ix = await buildCreateIssuerInstruction();
      const computeIx = web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 });

      const tx = new web3.Transaction().add(computeIx, ix);
      tx.recentBlockhash = (await rpc.getLatestBlockhash()).blockhash;
      tx.feePayer = issuer.publicKey;
      tx.sign(issuer);

      const sig = await rpc.sendTransaction(tx, [issuer]);
      await confirmTx(rpc, sig);

      console.log("Issuer created, tx:", sig);

      const slot = await rpc.getSlot();
      await rpc.confirmTransactionIndexed(slot);

      const accounts = await rpc.getCompressedAccountsByOwner(PROGRAM_ID);
      assert.ok(accounts.items.length > 0, "Issuer account should be created");
    });
  });

  describe("Credential lifecycle", () => {
    it("should generate credential keypair correctly", () => {
      const { privateKey, publicKey } = generateCredentialKeypair();

      const hash = poseidon([BigInt("0x" + Buffer.from(privateKey).toString("hex"))]);
      const computedPublicKey = bigintToBytes32(poseidon.F.toObject(hash));

      assert.deepStrictEqual(
        Array.from(publicKey),
        Array.from(computedPublicKey),
        "Public key should be Poseidon(privateKey)"
      );
    });

    it("should compute nullifier correctly", () => {
      const { privateKey } = generateCredentialKeypair();
      const verificationId = generateFieldElement();
      const nullifier = computeNullifier(verificationId, privateKey);

      const hash = poseidon([
        BigInt("0x" + Buffer.from(verificationId).toString("hex")),
        BigInt("0x" + Buffer.from(privateKey).toString("hex")),
      ]);
      const computedNullifier = bigintToBytes32(poseidon.F.toObject(hash));

      assert.deepStrictEqual(
        Array.from(nullifier),
        Array.from(computedNullifier),
        "Nullifier should be Poseidon(verification_id, privateKey)"
      );
    });
  });

  describe("ZK credential verification", () => {
    it("should demonstrate full ZK credential proof flow", async () => {
      const { privateKey: credentialPrivateKey, publicKey: credentialPubkey } = generateCredentialKeypair();

      const ownerHashed = hashToBn254Field(PROGRAM_ID.toBytes());
      const merkleTreeHashed = hashToBn254Field(
        new web3.PublicKey(defaultTestStateTreeAccounts().merkleTree).toBytes()
      );
      const issuerHashed = hashToBn254Field(issuer.publicKey.toBytes());

      const discriminator = new Uint8Array(32);
      discriminator.set(Buffer.from([0x2e, 0x9c, 0x4a, 0x87, 0x12, 0x34, 0x56, 0x78]), 24);

      const verificationId = generateFieldElement();
      const nullifier = computeNullifier(verificationId, credentialPrivateKey);

      const encryptedDataHash = generateFieldElement();
      const address = generateFieldElement();
      const pathElements = Array.from({ length: MERKLE_TREE_DEPTH }, () => new Uint8Array(32));

      const credentialDataHash = computeCredentialDataHash(issuerHashed, credentialPubkey);

      const LAMPORTS_OFFSET = 36893488147419103232n;
      const accountHash = poseidon([
        BigInt("0x" + Buffer.from(ownerHashed).toString("hex")),
        0n,
        BigInt("0x" + Buffer.from(merkleTreeHashed).toString("hex")),
        BigInt("0x" + Buffer.from(address).toString("hex")),
        BigInt("0x" + Buffer.from(discriminator).toString("hex")) + LAMPORTS_OFFSET,
        BigInt("0x" + Buffer.from(credentialDataHash).toString("hex")),
      ]);

      let current = poseidon.F.toObject(accountHash);
      for (let i = 0; i < MERKLE_TREE_DEPTH; i++) {
        const pathElement = BigInt("0x" + Buffer.from(pathElements[i]).toString("hex"));
        current = poseidon.F.toObject(poseidon([current, pathElement]));
      }
      const expectedRoot = bigintToBytes32(current);

      const zkProof = await generateCredentialProof({
        ownerHashed,
        merkleTreeHashed,
        discriminator,
        issuerHashed,
        expectedRoot,
        verificationId,
        publicEncryptedDataHash: encryptedDataHash,
        nullifier,
        credentialPrivateKey,
        leafIndex: 0,
        accountLeafIndex: 0,
        address,
        pathElements,
        encryptedDataHash,
      });

      assert.ok(zkProof.a.length === 32, "Proof A should be 32 bytes");
      assert.ok(zkProof.b.length === 64, "Proof B should be 64 bytes");
      assert.ok(zkProof.c.length === 32, "Proof C should be 32 bytes");
    });

    it("should verify nullifier uniqueness property", () => {
      const { privateKey } = generateCredentialKeypair();

      const verificationId1 = generateFieldElement();
      const verificationId2 = generateFieldElement();

      const nullifier1 = computeNullifier(verificationId1, privateKey);
      const nullifier2 = computeNullifier(verificationId2, privateKey);

      assert.notDeepStrictEqual(
        Array.from(nullifier1),
        Array.from(nullifier2),
        "Different verification IDs should produce different nullifiers"
      );
    });
  });
});
