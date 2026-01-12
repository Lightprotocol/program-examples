# Merkle Proof Circuit

Zero-knowledge circuit that proves a compressed account exists in a Merkle tree without revealing private account details.

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
- `owner_hashed` - Hash of program ID that owns the account
- `merkle_tree_hashed` - Hash of state merkle tree pubkey
- `discriminator` - Account type discriminator
- `data_hash` - Hash of account data
- `expectedRoot` - Merkle tree root

**Private inputs** (hidden):
- `leaf_index` - Position in merkle tree
- `account_leaf_index` - Leaf index field inside compressed account
- `address` - Account address
- `pathElements[26]` - Merkle proof path

## Circuit Structure

Single file `merkle_proof.circom` contains all templates:

```
AccountMerkleProof (main)
├── CompressedAccountHash
│   └── Poseidon hash of 6 account fields
└── MerkleProof (Tornado Cash Nova pattern)
    └── 26-level binary tree verification using Switcher
```
