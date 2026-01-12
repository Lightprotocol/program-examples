# Nullifiers for ZK Applications

Example to create one or four nullifiers. Uses Groth16 proofs and compressed accounts.

The required property for nullifiers is that they can not be created twice.
Light uses rent-free PDAs to track nullifiers in an address Merkle tree.
The address tree is the nullifier set and indexed by Helius.
You don't need to index your own Merkle tree.

On Solana, you typically would create a PDA account.
Nullifier accounts must remain active, hence lock ~0.001 SOL in rent per nullifier PDA permanently.

| Storage | Cost per nullifier |
|---------|-------------------|
| PDA | ~0.001 SOL |
| Compressed PDA | ~0.000005 SOL |

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

## Flow

1. Client computes nullifiers from secrets
2. Client generates Groth16 proof
3. On-chain: verify proof, derive addresses, create accounts
4. If any address exists, tx fails

## Program instructions

### 1. `create_nullifier`

Creates a single nullifier account using a ZK proof. The nullifier is derived from `Poseidon(verification_id, secret)` where only the prover knows the secret.

**Properties:**

- The secret stays private
- The nullifier is deterministic from the secret and verification_id
- If the nullifier address already exists, the transaction fails

### 2. `create_batch_nullifier`

Creates four nullifier accounts with a single ZK proof. Each nullifier is derived from the same `verification_id` but different secrets.

**Properties:**

- All four secrets stay private
- Single proof verification is ~2.7x more efficient per nullifier than four separate proofs
- If any nullifier address already exists, the entire transaction fails

## Compute units

| Method | Nullifiers | CU | CU per nullifier |
|--------|-----------|-------------|------------------|
| Single | 1 | ~237k | ~237k |
| Batch | 4 | ~350k* | ~88k |

*Estimated. Batch is ~2.7x more efficient per nullifier.

## Circuits

### Single nullifier (`nullifier.circom`)

**Public inputs:**

- `verification_id` - Context identifier (vote ID, airdrop ID, etc.)
- `nullifier` - The nullifier hash

**Private inputs:**

- `secret` - Only the owner knows this value

**Constraint:**

```circom
nullifier === Poseidon(verification_id, secret)
```

### Batch nullifier (`batchnullifier.circom`)

**Public inputs:**

- `verification_id` - Shared context for all nullifiers
- `nullifier[4]` - Array of 4 nullifier hashes

**Private inputs:**

- `secret[4]` - Array of 4 secrets

**Constraint:**

```circom
for i in 0..4:
    nullifier[i] === Poseidon(verification_id, secret[i])
```

## Setup

```bash
./scripts/setup.sh
```

This script will:

1. Check dependencies (Node.js, circom)
2. Install npm dependencies
3. Create build directories
4. Download the Powers of Tau ceremony file (14 powers)
5. Compile single nullifier circuit
6. Generate single nullifier proving key
7. Compile batch nullifier circuit
8. Generate batch nullifier proving key
9. Clean intermediate files

## Build and test

**Build:**

```bash
cargo build-sbf
```

**Rust tests:**

```bash
cargo test-sbf
```

**TypeScript tests:**

```bash
npm install
npm test
```

## Structure

```
zk-nullifier/
├── circuits/
│   ├── nullifier.circom        # Single
│   └── batchnullifier.circom   # Batch (4x)
├── src/lib.rs
├── tests/
│   ├── test.rs                 # Rust tests
│   └── zk-nullifier.ts         # TypeScript tests
└── scripts/setup.sh
```

## Cleaning build artifacts

To clean generated circuit files:

```bash
./scripts/clean.sh
```
