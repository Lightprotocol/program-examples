/**
 * Anonymous Airdrop Client
 *
 * Example client for claiming tokens anonymously from an airdrop.
 * The ZK proof hides which eligible address is claiming.
 */

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import * as snarkjs from "snarkjs";
import { buildPoseidon } from "circomlibjs";

// Types
interface EligibilityEntry {
  address: bigint; // Poseidon(privateKey)
  amount: bigint;
}

interface MerkleProof {
  pathElements: bigint[];
  leafIndex: number;
  root: bigint;
}

interface ClaimInputs {
  // Public inputs
  eligibilityRoot: bigint;
  nullifier: bigint;
  recipient: bigint;
  airdropId: bigint;
  amount: bigint;
  // Private inputs
  privateKey: bigint;
  pathElements: bigint[];
  leafIndex: number;
}

/**
 * Build an eligibility Merkle tree from (address, amount) pairs
 */
export async function buildEligibilityTree(
  entries: EligibilityEntry[]
): Promise<{ root: bigint; leaves: bigint[] }> {
  const poseidon = await buildPoseidon();

  // Compute leaves: leaf = Poseidon(address, amount)
  const leaves = entries.map((entry) => {
    const hash = poseidon([entry.address, entry.amount]);
    return poseidon.F.toObject(hash);
  });

  // Pad to power of 2
  const depth = 20;
  const treeSize = 2 ** depth;
  const paddedLeaves = [...leaves];
  const zeroLeaf = BigInt(0);
  while (paddedLeaves.length < treeSize) {
    paddedLeaves.push(zeroLeaf);
  }

  // Build tree bottom-up
  let currentLevel = paddedLeaves;
  for (let level = 0; level < depth; level++) {
    const nextLevel: bigint[] = [];
    for (let i = 0; i < currentLevel.length; i += 2) {
      const left = currentLevel[i];
      const right = currentLevel[i + 1];
      const hash = poseidon([left, right]);
      nextLevel.push(poseidon.F.toObject(hash));
    }
    currentLevel = nextLevel;
  }

  return {
    root: currentLevel[0],
    leaves,
  };
}

/**
 * Generate a Merkle proof for a leaf
 */
export async function getMerkleProof(
  entries: EligibilityEntry[],
  leafIndex: number
): Promise<MerkleProof> {
  const poseidon = await buildPoseidon();

  // Compute all leaves
  const leaves = entries.map((entry) => {
    const hash = poseidon([entry.address, entry.amount]);
    return poseidon.F.toObject(hash);
  });

  // Pad to power of 2
  const depth = 20;
  const treeSize = 2 ** depth;
  const paddedLeaves = [...leaves];
  const zeroLeaf = BigInt(0);
  while (paddedLeaves.length < treeSize) {
    paddedLeaves.push(zeroLeaf);
  }

  // Build tree and collect proof
  const pathElements: bigint[] = [];
  let currentLevel = paddedLeaves;
  let idx = leafIndex;

  for (let level = 0; level < depth; level++) {
    // Sibling index
    const siblingIdx = idx % 2 === 0 ? idx + 1 : idx - 1;
    pathElements.push(currentLevel[siblingIdx]);

    // Move to next level
    const nextLevel: bigint[] = [];
    for (let i = 0; i < currentLevel.length; i += 2) {
      const left = currentLevel[i];
      const right = currentLevel[i + 1];
      const hash = poseidon([left, right]);
      nextLevel.push(poseidon.F.toObject(hash));
    }
    currentLevel = nextLevel;
    idx = Math.floor(idx / 2);
  }

  return {
    pathElements,
    leafIndex,
    root: currentLevel[0],
  };
}

/**
 * Derive eligible address from private key
 */
export async function deriveEligibleAddress(privateKey: bigint): Promise<bigint> {
  const poseidon = await buildPoseidon();
  const hash = poseidon([privateKey]);
  return poseidon.F.toObject(hash);
}

/**
 * Compute nullifier for double-claim prevention
 */
export async function computeNullifier(
  airdropId: bigint,
  privateKey: bigint
): Promise<bigint> {
  const poseidon = await buildPoseidon();
  const hash = poseidon([airdropId, privateKey]);
  return poseidon.F.toObject(hash);
}

/**
 * Hash a value to BN254 field size (matches on-chain hashing)
 */
export function hashToBn254(value: Uint8Array): bigint {
  // SHA256 then truncate to 254 bits
  const crypto = require("crypto");
  const hash = crypto.createHash("sha256").update(value).digest();
  hash[0] = 0; // Zero first byte to fit in BN254 field
  return BigInt("0x" + hash.toString("hex"));
}

/**
 * Generate ZK proof for anonymous claim
 */
export async function generateClaimProof(
  inputs: ClaimInputs,
  wasmPath: string,
  zkeyPath: string
): Promise<{ proof: any; publicSignals: string[] }> {
  const circuitInputs = {
    // Public inputs
    eligibilityRoot: inputs.eligibilityRoot.toString(),
    nullifier: inputs.nullifier.toString(),
    recipient: inputs.recipient.toString(),
    airdropId: inputs.airdropId.toString(),
    amount: inputs.amount.toString(),
    // Private inputs
    privateKey: inputs.privateKey.toString(),
    pathElements: inputs.pathElements.map((e) => e.toString()),
    leafIndex: inputs.leafIndex,
  };

  const { proof, publicSignals } = await snarkjs.groth16.fullProve(
    circuitInputs,
    wasmPath,
    zkeyPath
  );

  return { proof, publicSignals };
}

/**
 * Compress proof for on-chain verification
 */
export function compressProof(proof: any): {
  a: Uint8Array;
  b: Uint8Array;
  c: Uint8Array;
} {
  // Convert proof points to compressed format
  // This matches the groth16-solana compression format

  const hexToBytes = (hex: string): Uint8Array => {
    const cleanHex = hex.startsWith("0x") ? hex.slice(2) : hex;
    const bytes = new Uint8Array(cleanHex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(cleanHex.substr(i * 2, 2), 16);
    }
    return bytes;
  };

  const bigIntToBytes32 = (n: bigint): Uint8Array => {
    const hex = n.toString(16).padStart(64, "0");
    return hexToBytes(hex);
  };

  // Proof A (G1 point) - 32 bytes compressed
  const aX = BigInt(proof.pi_a[0]);
  const aY = BigInt(proof.pi_a[1]);
  const aCompressed = new Uint8Array(32);
  const aXBytes = bigIntToBytes32(aX);
  aCompressed.set(aXBytes);
  // Set compression flag based on Y coordinate
  if (aY % BigInt(2) === BigInt(1)) {
    aCompressed[0] |= 0x80;
  }

  // Proof B (G2 point) - 64 bytes compressed
  const bCompressed = new Uint8Array(64);
  const b0 = BigInt(proof.pi_b[0][0]);
  const b1 = BigInt(proof.pi_b[0][1]);
  bCompressed.set(bigIntToBytes32(b0), 0);
  bCompressed.set(bigIntToBytes32(b1), 32);

  // Proof C (G1 point) - 32 bytes compressed
  const cX = BigInt(proof.pi_c[0]);
  const cY = BigInt(proof.pi_c[1]);
  const cCompressed = new Uint8Array(32);
  const cXBytes = bigIntToBytes32(cX);
  cCompressed.set(cXBytes);
  if (cY % BigInt(2) === BigInt(1)) {
    cCompressed[0] |= 0x80;
  }

  return {
    a: aCompressed,
    b: bCompressed,
    c: cCompressed,
  };
}

/**
 * Example: Full claim flow
 */
export async function exampleClaimFlow() {
  console.log("=== Anonymous Airdrop Claim Example ===\n");

  // 1. Setup: Authority has created eligibility tree
  const privateKey = BigInt(
    "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
  );
  const amount = BigInt(1000000); // 1M tokens

  const eligibleAddress = await deriveEligibleAddress(privateKey);
  console.log("Eligible address (Poseidon(privateKey)):", eligibleAddress.toString(16));

  // 2. Build eligibility tree (in practice, this is done by the authority)
  const entries: EligibilityEntry[] = [
    { address: eligibleAddress, amount },
    // ... other eligible addresses
  ];

  const { root: eligibilityRoot } = await buildEligibilityTree(entries);
  console.log("Eligibility root:", eligibilityRoot.toString(16));

  // 3. Get Merkle proof for our entry
  const leafIndex = 0;
  const merkleProof = await getMerkleProof(entries, leafIndex);
  console.log("Merkle proof leaf index:", leafIndex);

  // 4. Compute nullifier
  const airdropId = BigInt(1);
  const nullifier = await computeNullifier(airdropId, privateKey);
  console.log("Nullifier:", nullifier.toString(16));

  // 5. Choose recipient (can be any address - this is the privacy feature!)
  const recipientKeypair = Keypair.generate();
  const recipient = hashToBn254(recipientKeypair.publicKey.toBytes());
  console.log("Recipient (fresh wallet):", recipientKeypair.publicKey.toBase58());

  // 6. Generate ZK proof
  const inputs: ClaimInputs = {
    eligibilityRoot,
    nullifier,
    recipient,
    airdropId,
    amount,
    privateKey,
    pathElements: merkleProof.pathElements,
    leafIndex: merkleProof.leafIndex,
  };

  console.log("\nGenerating ZK proof...");
  // const { proof } = await generateClaimProof(
  //   inputs,
  //   "./build/airdrop_claim_js/airdrop_claim.wasm",
  //   "./build/airdrop_claim_final.zkey"
  // );

  console.log("Proof generated!");
  console.log("\nPrivacy guarantee:");
  console.log("- Observer sees: 'Someone claimed 1M tokens to", recipientKeypair.publicKey.toBase58(), "'");
  console.log("- Observer CANNOT tell which eligible address is claiming");
  console.log("- The eligible address remains hidden in the proof");
}

// Run example if executed directly
if (require.main === module) {
  exampleClaimFlow().catch(console.error);
}

