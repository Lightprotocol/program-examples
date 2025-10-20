# Compressed Account Merkle Proof Circuit

Zero-knowledge circuit that proves ownership of a compressed account in a Merkle tree without revealing the account details.

## What It Does

The circuit verifies:
1. **Account Hash** - Computes Poseidon hash of account fields (owner, discriminator, data)
2. **Merkle Inclusion** - Proves the account exists at a specific leaf in a 26-level tree

## Setup & Testing

```bash
# Compile circuit and generate keys
./scripts/setup.sh

# Run tests
cargo test-sbf

# Clean build artifacts
./scripts/clean.sh
```

## Circuit I/O

**Public inputs** (visible in proof):
- `owner_hashed`, `merkle_tree_hashed`, `discriminator` - Account identifiers
- `issuer_hashed` - Credential issuer
- `expectedRoot` - Merkle tree root
- `public_encrypted_data_hash`, `public_data_hash` - Data commitments

**Private inputs** (hidden):
- `leaf_index` - Account position in tree
- `pathElements[26]` - Merkle proof path

## Architecture

```
CompressedAccountMerkleProof
├── CompressedAccountHash (Poseidon hash of 5 fields)
└── MerkleProof (26-level binary tree verification)
