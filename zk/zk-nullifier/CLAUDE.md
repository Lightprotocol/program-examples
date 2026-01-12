# Nullifier

Nullifier for ZK applications with Groth16 verification and compressed accounts.

## Summary

- Nullifier = `Poseidon(verification_id, secret)`
- If nullifier account exists, tx fails
- Max per tx: 1 (single) or 4 (batch)

## Instructions

| Instruction | Public Inputs | Nullifiers |
|-------------|--------------|------------|
| `create_nullifier` | 2 (vid, nullifier) | 1 |
| `create_batch_nullifier` | 5 (vid, 4 nullifiers) | 4 |

## Circuits

```
circuits/
├── nullifier.circom        # Single: ~237k CU
└── batchnullifier.circom   # Batch 4x: ~350k CU (~88k per nullifier)
```

## Accounts

| Account | Seeds |
|---------|-------|
| `NullifierAccount` | `[b"nullifier", nullifier, verification_id]` |

## Build & Test

```bash
./scripts/setup.sh
cargo build-sbf && cargo test-sbf  # Rust tests (CU measurement)
npm install && npm test            # TypeScript tests (requires local validator)
```

Rust tests print actual CU consumed. TypeScript tests use snarkjs for proof generation.

## Errors

| Code | Name |
|------|------|
| 6000 | `AccountNotEnoughKeys` |
