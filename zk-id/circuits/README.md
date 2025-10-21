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
- `verification_id` - Context for nullifier generation (prevents reuse in same context)
- `public_encrypted_data_hash` - Encrypted data commitment
- `nullifier` - Unique value preventing double-spending (Poseidon(verification_id, credential_secret))

**Private inputs** (hidden):
- `credentialPrivateKey` - Secret key proving credential ownership
- `leaf_index`, `account_leaf_index` - Account positions
- `address` - Account address
- `pathElements[26]` - Merkle proof path
- `encrypted_data_hash` - Private data hash

## Circuit Files

- `compressed_account_merkle_proof.circom` - Main circuit that combines all components
- `credential.circom` - Keypair verification for credential ownership
- `compressed_account.circom` - Computes Poseidon hash of account fields
- `merkle_proof.circom` - Binary Merkle tree inclusion proof

## Architecture

```
CompressedAccountMerkleProof (main)
├── Keypair (credential.circom)
│   └── Proves knowledge of private key
├── CompressedAccountHash (compressed_account.circom)
│   └── Poseidon hash of 6 fields
└── MerkleProof (merkle_proof.circom)
    └── 26-level binary tree verification
