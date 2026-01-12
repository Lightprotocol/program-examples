# ZK Private Payments

Confidential token transfers on Solana using ZK proofs and Light Protocol compressed accounts.

**Educational only. Not audited.**

## Overview

Private payment system where token amounts are hidden:
- **Deposit**: Convert public tokens → private balance commitment
- **Transfer**: Send tokens privately (hidden amounts)
- **Withdraw**: Convert private balance → public tokens

## How It Works

### Balance Commitments

Balances are stored as Poseidon commitments:
```
commitment = Poseidon(amount, blinding)
```

Only the owner knows the preimage (amount + blinding factor).

### Private Transfer

The ZK proof verifies:
1. Sender knows the preimage of their commitment
2. Sender has sufficient balance (amount >= transfer_amount)
3. New commitments are computed correctly
4. Nullifier prevents double-spend

Observers see only commitments, not actual amounts.

## Requirements

- **Rust** (1.90.0+)
- **Node.js** (v22+)
- **Solana CLI** (2.3.11+)
- **Light CLI**: `npm install -g @lightprotocol/zk-compression-cli`
- **Circom** (v2.2.2)
- **SnarkJS**: `npm install -g snarkjs`

## Setup

```bash
./scripts/setup.sh
```

## Build and Test

```bash
cargo build-sbf
cargo test-sbf
```

## Project Structure

```
confidential-transfer/
├── circuits/
│   ├── compressed_account.circom  # Account hash computation
│   ├── merkle_proof.circom        # Merkle tree verification
│   ├── range_check.circom         # Balance sufficiency checks
│   └── private_transfer.circom    # Main transfer circuit
├── scripts/
│   ├── setup.sh                   # Circuit compilation
│   └── clean.sh                   # Remove artifacts
├── src/
│   ├── lib.rs                     # Solana program
│   └── verifying_key.rs           # Generated VK
├── tests/
│   └── test.rs                    # Integration tests
└── README.md
```

## Circuit Public Inputs

1. `owner_hashed` - Program ID hashed to BN254 field
2. `merkle_tree_hashed` - State tree pubkey hashed
3. `discriminator` - Account type discriminator
4. `mint_hashed` - Token mint hashed
5. `expectedRoot` - Merkle root
6. `nullifier` - Prevents double-spend
7. `receiver_commitment` - Receiver's balance commitment

## License

MIT
