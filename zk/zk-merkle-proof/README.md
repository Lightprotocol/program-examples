# Minimal Merkle Proof Example

Proves compressed account existence without revealing the Merkle path or leaf position.

The prover knows which account exists in the state tree. The verifier only learns that some account with the given data hash exists - not where it is or how to find it.

## Program Instructions

### 1. `create_account`

Creates a compressed account with a data hash. The account is stored in Light Protocol's state Merkle tree.

### 2. `verify_account`

Verifies a Groth16 proof that the account exists in the state tree. Does not modify state - only checks the proof against the current Merkle root.

**Properties:**

- The Merkle path (26 siblings) stays private
- The leaf position stays private
- The account address stays private
- Only the data hash, discriminator, and Merkle root are public

## Requirements

### System dependencies

- **Rust** (1.90.0 or later)
- **Node.js** (v22 or later) and npm
- **Solana CLI** (2.3.11 or later)
- **Light CLI**: Install with `npm install -g @lightprotocol/zk-compression-cli`

### ZK circuit tools

- **Circom** (v2.2.2): Zero-knowledge circuit compiler
- **SnarkJS**: JavaScript library for generating and verifying ZK proofs

To install circom and snarkjs:

```bash
# Install circom (Linux/macOS)
wget https://github.com/iden3/circom/releases/download/v2.2.2/circom-linux-amd64
chmod +x circom-linux-amd64
sudo mv circom-linux-amd64 /usr/local/bin/circom

# For macOS, replace with circom-macos-amd64

# Install snarkjs globally
npm install -g snarkjs
```

## Circuit

### Public inputs

- `owner_hashed` - Hash of the program ID owning the account
- `merkle_tree_hashed` - Hash of the state tree pubkey
- `discriminator` - Account type discriminator
- `data_hash` - Hash of account data
- `expectedRoot` - Current Merkle root

### Private inputs

- `leaf_index` - Position of the account in the Merkle tree
- `account_leaf_index` - Leaf index field inside compressed account hash
- `address` - Compressed account address
- `pathElements[26]` - Merkle proof siblings

### Constraint

The circuit:

1. Computes the account hash: `Poseidon(owner_hashed, leaf_index, merkle_tree_hashed, address, discriminator + DOMAIN, data_hash)`
2. Verifies the Merkle proof against `expectedRoot`

## Setup

```bash
./scripts/setup.sh
```

This script will:

1. Check dependencies (Node.js, circom)
2. Install npm dependencies
3. Create build directories
4. Download the Powers of Tau ceremony file (16 powers)
5. Compile the circuit
6. Generate the proving key (zkey) with contribution
7. Export the verification key

## Build and Test

**Build:**

```bash
cargo build-sbf
```

**Rust tests:**

```bash
cargo test-sbf -- --nocapture
```

## Structure

```
zk-merkle-proof/
├── circuits/
│   └── merkle_proof.circom          # All circuit templates (Tornado Cash Nova pattern)
├── src/
│   └── lib.rs                       # Solana program with Groth16 verification
├── tests/
│   └── test.rs                      # Rust integration tests
└── scripts/
    └── setup.sh                     # Circuit compilation and setup
```

## Cleaning build artifacts

To clean generated circuit files:

```bash
./scripts/clean.sh
```
