
# ZK-ID Program

A minimal zk id Solana program that uses zero-knowledge proofs for identity verification with compressed accounts.
Note this is an example how to verify a zk inclusion proof, not a full zk identity protocol and not production-ready.

For examples of zk identity protocols, see:
- [Iden3](https://github.com/iden3) - Full decentralized identity protocol with claims, revocation, and recovery
- [Semaphore](https://github.com/semaphore-protocol/semaphore) - Privacy-preserving group signaling with nullifiers

## Program Instructions

### 1. `create_issuer`
Creates a compressed account for an issuer entity who can credential other users, storing their pubkey and initializing their credential issuance counter.

### 2. `add_credential`
Issues a new credential by creating a compressed account that binds a user's pubkey to an issuer, incrementing the issuer's credential counter in the process.

### 3. `zk_verify_credential`
Verifies a zero-knowledge proof of credential ownership using Groth16 verification and creates an encrypted event account to store the verification result on-chain.

**Properties:**
- Credential verification is private. The credential is not exposed during zk proof verification.
  (The transaction payer is not private, for full privacy a relayer or freshly funded keypair should be used.)
- Each credential can only be used once per `verification_id`. (The event account address serves as a nullifier.)
- Only the credential owner can produce a valid proof.

## Requirements

### System Dependencies
- **Rust** (1.90.0 or later)
- **Node.js** (v22 or later) and npm
- **Solana CLI** (2.3.11 or later)
- **Light CLI**: Install with `npm install -g @lightprotocol/zk-compression-cli`

### ZK Circuit Tools
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

## Setup

Before building and testing, you need to compile the ZK circuits and generate the proving/verification keys:

```bash
# Run the setup script to compile circuits and generate keys
./scripts/setup.sh
```

This script will:
1. Install npm dependencies
2. Download the Powers of Tau ceremony file
3. Compile the circom circuit
4. Generate the proving key (zkey)
5. Export the verification key

## Build and Test

### Using Makefile

From the parent `zk/` directory:

```bash
# Build, deploy, and test this example
make zk-id

# Or run individual steps
make build      # Build all programs
make deploy     # Deploy to local validator
make test-ts    # Run TypeScript tests
```

### Manual commands

**Build:**
```bash
cargo build-sbf
```

**Rust tests** (full ZK verification flow):
```bash
RUST_BACKTRACE=1 cargo test-sbf -- --nocapture
```

**TypeScript tests:**

Requires a running local validator with Light Protocol:
```bash
light test-validator  # In separate terminal
npm install
npm run test:ts
```

## Structure

```
zk-id/
├── circuits/                 # Circom circuit definitions
│   └── compressed_account_merkle_proof.circom
├── build/                   # Generated circuit artifacts (after setup)
│   ├── verification_key.json
│   └── *.zkey, *.wasm, etc.
├── scripts/
│   └── setup.sh            # Circuit compilation and setup script
├── src/
│   ├── lib.rs             # Solana program implementation
│   └── verifying_key.rs   # Generated Groth16 verifying key
├── tests/
│   └── test.rs            # Rust integration tests
└── ts-tests/
    └── zk-id.test.ts      # TypeScript tests
```

## Light Protocol V2 API

This example uses Light SDK v0.17+ with the V2 accounts layout:

- `system_accounts_offset` parameter to locate system accounts in remaining accounts
- `CpiAccounts::new()` from `light_sdk::cpi::v2`
- `into_new_address_params_assigned_packed(seed, Some(index))` for address parameters
- `sha::LightAccount` for accounts with Vec fields (uses SHA256 flat hashing)
- `poseidon::LightAccount` for accounts with fixed-size fields (uses Poseidon hashing)

## Cleaning Build Artifacts

To clean generated circuit files:
```bash
./scripts/clean.sh
```
