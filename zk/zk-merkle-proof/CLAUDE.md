# zk-merkle-proof

Proves compressed account existence with Groth16 verification without revealing Merkle path.

## Summary

- Creates compressed accounts with Poseidon-hashed data
- Verifies Groth16 proofs that account exists in state tree
- Merkle path, leaf position, and address stay private
- Only data_hash, discriminator, and root are public inputs

## Instructions

### `create_account`

Creates a compressed account with a data hash.

- **path**: `src/lib.rs:37`

**Instruction data:**

| Field | Type | Description |
|-------|------|-------------|
| `proof` | `ValidityProof` | Light Protocol validity proof |
| `address_tree_info` | `PackedAddressTreeInfo` | Address tree reference |
| `output_state_tree_index` | `u8` | State tree for output |
| `data_hash` | `[u8; 32]` | Hash of account data |

**Accounts:**

| Name | Signer | Writable | Description |
|------|--------|----------|-------------|
| `signer` | ✓ | ✓ | Transaction fee payer |

**Logic:**

1. Derive address from `[b"data_account", data_hash]`
2. Create `DataAccount` with Poseidon hashing via Light CPI

### `verify_account`

Verifies a Groth16 proof that account exists in state tree.

- **path**: `src/lib.rs:76`

**Instruction data:**

| Field | Type | Description |
|-------|------|-------------|
| `input_root_index` | `u16` | Root index in state tree |
| `zk_proof` | `CompressedProof` | Groth16 proof (a, b, c) |
| `data_hash` | `[u8; 32]` | Expected data hash |

**Accounts:**

| Name | Signer | Writable | Description |
|------|--------|----------|-------------|
| `signer` | ✓ | ✓ | Transaction fee payer |
| `state_merkle_tree` | ✗ | ✗ | State tree to read root from |

**Logic:**

1. Read expected root from state Merkle tree at `input_root_index`
2. Hash program ID and tree pubkey to BN254 field size
3. Construct 5 public inputs: `[owner_hashed, merkle_tree_hashed, discriminator, data_hash, expected_root]`
4. Decompress G1/G2 proof points
5. Verify Groth16 proof against verifying key

## Accounts

### `DataAccount`

Stores a data hash using Poseidon hashing.

- **path**: `src/lib.rs:157`
- **derivation**: `[b"data_account", data_hash]`
- **hashing**: Poseidon (via `LightHasher` derive)

## Circuit

5 public inputs:

- `owner_hashed` - program ID hashed to BN254 field
- `merkle_tree_hashed` - state tree pubkey hashed
- `discriminator` - account type discriminator
- `data_hash` - account data hash
- `expectedRoot` - current Merkle root

Private inputs (hidden in proof):

- `leaf_index` - position in tree
- `account_leaf_index` - SDK internal position
- `address` - account address
- `pathElements[26]` - Merkle proof siblings

## Build & Test

```bash
./scripts/setup.sh                    # Compile circuits, generate zkeys
cargo build-sbf && cargo test-sbf     # Rust tests
```
