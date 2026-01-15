# Nullifier for ZK 

Program with a single instruction to create nullifiers to prevent double-spending.

* Nullifiers require a data structure that ensures the nullifier is only created once.
* On Solana a straight forward way is to derive a PDA with the nullifier as seed for the PDA account.
* Nullifier accounts must remain active, hence lock ~0.001 SOL in rent per nullifier PDA permanently.
* Compressed pdas are rent-free, provide similar functionality and derivation.

| Storage | Cost per nullifier |
|---------|-------------------|
| PDA | ~0.001 SOL |
| Compressed PDA | ~0.000015 SOL |

In detail, a nullifier is a hash derived from your secret and the leaf the transaction is using.
When you use private state (stored in a Merkle tree leaf), you publish the nullifier to invalidate the state to prevent double spending. The program stores all nullifiers in a set.
If anyone tries to spend the same leaf again, the nullifier would match one already stored, so the transaction fails.
The nullifier reveals nothing about which leaf was spent.
Different state produces different nullifiers, so observers can't link a nullifier back to its source leaf.

## Flow
1. Client computes nullifier values (typically `hash(secret, context)`) and fetches validity proof from RPC for the derived addresses to prove it does not exist.
3. Client calls `create_nullifier` with data, nullifiers and validity proof
4. Program derives addresses, creates compressed accounts via CPI to Light system program
5. If any address exists, Light system program rejects the CPI

## Build and Test

- **Rust** (1.90.0 or later)
- **Node.js** (v22 or later)
- **Solana CLI** (2.3.11 or later)
- **Light CLI**: `npm install -g @lightprotocol/zk-compression-cli`
### Using Makefile

From the parent `zk/` directory:

```bash
make nullifier    # Build, deploy, test
make build           # Build all programs
make deploy          # Deploy to local validator
make test-ts         # Run TypeScript tests
```

### Manual commands

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
light test-validator  # In separate terminal
npm install
npm run test:ts
```

## Structure

```
nullifier/
├── programs/nullifier/
│   ├── src/lib.rs           # Program with create_nullifiers helper
│   └── tests/test.rs        # Rust integration tests
└── ts-tests/
    └── nullifier.test.ts    # TypeScript tests
```
