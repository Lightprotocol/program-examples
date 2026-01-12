# Mixer Circuits

Circom circuits for the Solana ZK mixer with Light Protocol integration.

## Files

- `withdraw.circom` - Main withdrawal circuit (26-level Poseidon Merkle tree)
- `merkletree.circom` - Poseidon-based Merkle tree proof verification

## Circuit Logic

The withdrawal circuit proves:

1. Knowledge of `secret` and `nullifier` such that `commitment = Poseidon(nullifier, secret)`
2. This commitment exists in the Merkle tree at the given `root`
3. The `nullifierHash` is correctly computed as `Poseidon(nullifier)`

## Public Inputs (6)

| Input | Description |
|-------|-------------|
| `root` | Merkle root proving commitment membership |
| `nullifierHash` | `Poseidon(nullifier)` - prevents double-spend |
| `recipient` | Withdrawal address (hashed to BN254 field) |
| `relayer` | Relayer address (for relayer support) |
| `fee` | Relayer fee amount |
| `refund` | Refund amount |

## Private Inputs

| Input | Description |
|-------|-------------|
| `nullifier` | BN254 field element (248 bits) |
| `secret` | BN254 field element (248 bits) |
| `pathElements[26]` | Merkle proof sibling hashes |
| `pathIndices[26]` | Left/right selector bits (0 or 1) |

## Constraints

1. `commitment = Poseidon(nullifier, secret)`
2. `nullifierHash = Poseidon(nullifier)` (verified against public input)
3. `MerkleTreeChecker(commitment, pathElements, pathIndices) == root`
4. Public inputs bound via quadratic constraints (prevents tampering)

## Setup

```bash
# Compile circuit and generate keys
./scripts/setup.sh

# Clean build artifacts
./scripts/clean.sh
```
