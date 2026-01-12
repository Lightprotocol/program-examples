
# ZK-Vote Program

An anonymous voting Solana program that uses zero-knowledge proofs with compressed accounts. Voters prove credential ownership without revealing their identity. Vote choices are public, voter identity is hidden.

Note: This is an example of ZK-based anonymous voting, not a production-ready voting system.

## Program Instructions

### 1. `create_poll`

Creates a Poll PDA with a question and 3 voting options. The authority who creates the poll can register voters.

### 2. `register_voter`

Registers an eligible voter by creating a compressed VoterCredential. Only the poll authority can register voters. The credential stores a Poseidon-hashed poll ID and credential pubkey for ZK compatibility.

### 3. `vote`

Submits a vote with a ZK proof of credential ownership. The proof verifies:

- The voter owns a valid credential for this poll
- The nullifier is correctly derived from the credential

Creates a VoteRecord at a nullifier-derived address (prevents double-voting) and increments the poll's vote count.

### 4. `close_poll`

Closes the poll and emits the winner.

## Privacy Properties

| Property | Value |
|----------|-------|
| Voter identity | Hidden (ZK proof hides which credential was used) |
| Vote choice | Public (visible on-chain) |
| Vote counts | Public (updated in real-time) |
| Double-vote prevention | Nullifier-derived address (cryptographic) |
| Trust model | Trustless (cryptographic proofs, no MPC nodes) |

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

Before building and testing, compile the ZK circuits and generate the proving/verification keys:

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

```bash
# Build the program
cargo build-sbf

# Run tests
RUST_BACKTRACE=1 cargo test-sbf -- --nocapture
```

## Structure

```
zk-vote/
├── circuits/
│   ├── vote_proof.circom           # Main voting circuit (26 levels for v1 trees)
│   ├── credential.circom           # Keypair derivation (Poseidon hash)
│   ├── merkle_proof.circom         # Merkle proof verification
│   └── compressed_account.circom   # Account hash computation
├── build/                          # Generated circuit artifacts (after setup)
│   ├── verification_key.json
│   └── *.zkey, *.wasm, etc.
├── scripts/
│   └── setup.sh                    # Circuit compilation and setup script
├── src/
│   ├── lib.rs                      # Program with Poll, VoterCredential, VoteRecord
│   └── verifying_key.rs            # Groth16 verification key
└── tests/
    ├── circuit.rs                  # Circuit unit tests
    └── test.rs                     # Integration tests
```

## Cleaning Build Artifacts

To clean generated circuit files:
```bash
./scripts/clean.sh
```
