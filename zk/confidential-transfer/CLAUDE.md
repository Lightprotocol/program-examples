# ZK Private Payments

Private token transfers using ZK proofs to hide payment amounts.

## Summary

- Deposit: Public tokens → private balance commitment = `Poseidon(amount, blinding)`
- Transfer: Send tokens privately (hidden amounts) with ZK proof
- Withdraw: Private balance → public tokens
- Nullifier prevents double-spend

## Quick Start

```bash
./scripts/setup.sh
cargo build-sbf
cargo test-sbf
```

## Source Structure

```
src/
├── lib.rs           # Program entry, instructions, accounts
└── verifying_key.rs # Groth16 verifying key (7 public inputs)

circuits/
├── private_transfer.circom   # Main transfer circuit
├── compressed_account.circom # Account hash computation
├── merkle_proof.circom       # Merkle tree verification (26 levels)
└── range_check.circom        # Balance sufficiency checks

tests/
└── test.rs          # E2E test with proof generation
```

## Instructions

| Instruction | Description |
|-------------|-------------|
| `initialize` | Create vault for token mint |
| `deposit` | Deposit tokens, create balance commitment |
| `create_balance_commitment` | Create commitment without token transfer (testing) |
| `transfer` | Private transfer with ZK proof |
| `withdraw` | Withdraw tokens by proving commitment ownership |

## Compressed Accounts

| Account | Seeds | Fields |
|---------|-------|--------|
| `BalanceCommitment` | `[b"balance", commitment]` | `mint_hashed`, `commitment` |
| `NullifierAccount` | `[b"nullifier", nullifier]` | `nullifier` |

## ZK Circuit (PrivateTransfer)

**Public inputs** (7 signals):
1. `owner_hashed` - Program ID hashed to BN254 field
2. `merkle_tree_hashed` - State tree pubkey hashed
3. `discriminator` - BalanceCommitment discriminator
4. `mint_hashed` - Token mint hashed
5. `expectedRoot` - Merkle tree root
6. `nullifier` - Prevents double-spend
7. `receiver_commitment` - Receiver's balance commitment

**Private inputs**:
- `sender_amount`, `sender_blinding` - Sender's balance preimage
- `transfer_amount` - Amount to transfer
- `new_sender_blinding`, `receiver_blinding` - New blinding factors
- `leaf_index`, `account_leaf_index`, `address` - Account position
- `pathElements[26]` - Merkle proof

**Circuit constraints**:
1. Verify sender knows preimage: `sender_commitment = Poseidon(sender_amount, sender_blinding)`
2. Verify nullifier: `nullifier = Poseidon(sender_commitment, sender_blinding)`
3. Range check: `sender_amount >= transfer_amount` (64-bit decomposition)
4. Verify new sender commitment: `new_sender_commitment = Poseidon(sender_amount - transfer_amount, new_sender_blinding)`
5. Verify receiver commitment: `receiver_commitment = Poseidon(transfer_amount, receiver_blinding)`
6. Compute data_hash and account hash
7. Verify Merkle proof against `expectedRoot`

## Errors

| Code | Name | Message |
|------|------|---------|
| 6000 | `ZeroAmount` | Zero amount |
| 6001 | `Overflow` | Arithmetic overflow |
| 6002 | `InsufficientFunds` | Insufficient funds in vault |
| 6003 | `AccountNotEnoughKeys` | Not enough keys in remaining accounts |
| 6004 | `InvalidProof` | Invalid proof |

## Dependencies

- Light Protocol SDK (compression, Merkle trees)
- groth16-solana (on-chain proof verification)
- circomlib (Poseidon, comparators)
