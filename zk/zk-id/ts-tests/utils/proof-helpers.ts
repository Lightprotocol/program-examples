/**
 * Shared proof helpers for ZK circuits using snarkjs.
 *
 * These utilities help convert snarkjs Groth16 proofs to the compressed format
 * expected by groth16-solana for on-chain verification.
 */

import BN from "bn.js";
import { utils } from "ffjavascript";
import { FIELD_SIZE } from "@lightprotocol/stateless.js";

const leInt2Buff = utils.leInt2Buff;
const unstringifyBigInts = utils.unstringifyBigInts;

/** Compressed Groth16 proof format for Solana */
export interface CompressedProof {
  a: number[];
  b: number[];
  c: number[];
}

/**
 * Convert snarkjs proof to compressed format for groth16-solana.
 *
 * This follows the prover.js logic from light-protocol for correct
 * byte ordering and compression with sign bits.
 *
 * @param proof - Raw snarkjs Groth16 proof object
 * @returns Compressed proof with 32-byte a, 64-byte b, 32-byte c
 */
export function parseProofToCompressed(proof: any): CompressedProof {
  // Clone and convert to LE bytes
  const mydata = JSON.parse(JSON.stringify(proof));

  // Convert pi_a and pi_c: LE bytes then reverse to BE
  for (const i of ["pi_a", "pi_c"]) {
    for (const j in mydata[i]) {
      mydata[i][j] = Array.from(
        leInt2Buff(unstringifyBigInts(mydata[i][j]), 32)
      ).reverse(); // LE to BE
    }
  }

  // Convert pi_b: just LE bytes (no reverse per element)
  for (const j in mydata.pi_b) {
    for (const z in mydata.pi_b[j]) {
      mydata.pi_b[j][z] = Array.from(
        leInt2Buff(unstringifyBigInts(mydata.pi_b[j][z]), 32)
      );
    }
  }

  // Compress proof_a: X coordinate with sign bit (negated for Solana verify)
  const proofA = mydata.pi_a[0];
  const proofAIsPositive = yElementIsPositiveG1(new BN(mydata.pi_a[1])) ? false : true;
  proofA[0] = addBitmaskToByte(proofA[0], proofAIsPositive);

  // Compress proof_b: X coordinate (flattened and reversed) with sign bit
  const proofB = mydata.pi_b[0].flat().reverse();
  const proofBY = mydata.pi_b[1].flat().reverse();
  const proofBIsPositive = yElementIsPositiveG2(
    new BN(proofBY.slice(0, 32)),
    new BN(proofBY.slice(32, 64))
  );
  proofB[0] = addBitmaskToByte(proofB[0], proofBIsPositive);

  // Compress proof_c: X coordinate with sign bit
  const proofC = mydata.pi_c[0];
  const proofCIsPositive = yElementIsPositiveG1(new BN(mydata.pi_c[1]));
  proofC[0] = addBitmaskToByte(proofC[0], proofCIsPositive);

  return { a: proofA, b: proofB, c: proofC };
}

/**
 * Check if y element is positive for G1 points (BN254 curve).
 * Used for determining the sign bit in point compression.
 */
function yElementIsPositiveG1(yElement: BN): boolean {
  return yElement.lte(FIELD_SIZE.sub(yElement));
}

/**
 * Check if y element is positive for G2 points (BN254 curve).
 * G2 points have 2 components for the y coordinate.
 */
function yElementIsPositiveG2(yElement1: BN, yElement2: BN): boolean {
  const fieldMidpoint = FIELD_SIZE.div(new BN(2));
  if (yElement1.lt(fieldMidpoint)) {
    return true;
  } else if (yElement1.gt(fieldMidpoint)) {
    return false;
  }
  return yElement2.lt(fieldMidpoint);
}

/**
 * Add sign bitmask to the first byte of a compressed point.
 * Compatible with Solana's altbn128 compression syscall and arkworks.
 */
function addBitmaskToByte(byte: number, yIsPositive: boolean): number {
  if (!yIsPositive) {
    return (byte |= 1 << 7);
  }
  return byte;
}

/** Convert BigInt to 32-byte big-endian array */
export function bigintToBytes32(n: bigint): Uint8Array {
  const hex = n.toString(16).padStart(64, "0");
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

/** Convert bytes to field string for circuit inputs */
export function toFieldString(bytes: Uint8Array): string {
  return BigInt("0x" + Buffer.from(bytes).toString("hex")).toString();
}

/** Generate random 32-byte value in BN254 field */
export function generateFieldElement(): Uint8Array {
  const value = new Uint8Array(32);
  crypto.getRandomValues(value);
  value[0] = 0; // Ensure value is in BN254 field (< 2^254)
  return value;
}
