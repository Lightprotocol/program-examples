# ZK Mixer

A ZK-SNARK mixer on Solana enabling private deposits and withdrawals using Light Protocol compressed accounts. Users deposit a fixed amount of SOL and can later withdraw to any address without revealing the link between deposit and withdrawal.

**Educational only. Not audited.**

## How It Works

### Deposit

1. User generates a random `secret` and `nullifier` (248 bits each)
2. Computes `commitment = Poseidon(nullifier, secret)`
3. Sends commitment to on-chain state tree along with SOL

### Withdrawal

1. User provides a ZK proof showing they know `secret` and `nullifier` for a commitment in the tree
2. Circuit verifies: commitment exists in tree, nullifier hash matches
3. Program verifies proof on-chain using Groth16 (BN254 pairing via Solana syscalls)
4. SOL sent to recipient, nullifier marked spent (prevents double-spend)

### Privacy

The ZK proof reveals nothing about which deposit is being withdrawn. An observer sees only that *some* valid deposit is being claimed.

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

# Install snarkjs globally
npm install -g snarkjs
```

## Setup

Before building and testing, compile the ZK circuits and generate proving/verification keys:

```bash
# Run the setup script
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

## Project Structure

```
mixer/
├── circuits/                 # Circom circuit definitions
│   ├── withdraw.circom       # Main withdrawal circuit
│   └── merkletree.circom     # Merkle tree verification
├── build/                    # Generated circuit artifacts (after setup)
│   ├── verification_key.json
│   └── *.zkey, *.wasm, etc.
├── scripts/
│   ├── setup.sh              # Circuit compilation and setup
│   └── clean.sh              # Remove build artifacts
├── src/
│   ├── lib.rs                # Solana program implementation
│   └── verifying_key.rs      # Groth16 verification key (auto-generated)
├── tests/
│   └── test.rs               # Integration tests
├── build.rs                  # VK generation from JSON
├── Cargo.toml
├── package.json
├── CLAUDE.md                 # Detailed technical documentation
└── README.md                 # This file
```

## Technical Details

### Circuit Public Inputs

1. `root` - Merkle root (proves membership)
2. `nullifierHash` - Poseidon(nullifier) (prevents double-spend)
3. `recipient` - Withdrawal address
4. `relayer`, `fee`, `refund` - For relayer support (optional)

### Proof Format

groth16-solana expects:
- `pi_a`: G1 point, y-coordinate negated
- `pi_b`: G2 point, coordinates in specific LE/BE format
- `pi_c`: G1 point

### BN254 Field Constraint

Values are limited to 248 bits to fit within BN254's scalar field. The first byte of nullifier/secret is always 0.

## Cleaning Build Artifacts

To clean generated circuit files:
```bash
./scripts/clean.sh
```

## Architecture

See [CLAUDE.md](CLAUDE.md) for detailed technical documentation including:
- Account structures and address derivation
- Instruction parameters and logic
- ZK circuit specification
- Security properties and checks

## License

MIT
