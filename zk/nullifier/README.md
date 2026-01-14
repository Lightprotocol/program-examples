# Nullifier Registry

Creates nullifier accounts using compressed addresses. Prevents double-spend by failing if a nullifier already exists.

Light uses rent-free PDAs in an address Merkle tree indexed by Helius. No custom indexing required.

| Storage | Cost per nullifier |
|---------|-------------------|
| PDA | ~0.001 SOL |
| Compressed PDA | ~0.000005 SOL |

## Requirements

- **Rust** (1.90.0 or later)
- **Node.js** (v22 or later)
- **Solana CLI** (2.3.11 or later)
- **Light CLI**: `npm install -g @lightprotocol/zk-compression-cli`

## Flow

1. Client derives nullifier addresses from `[NULLIFIER_PREFIX, nullifier_value]`
2. Client requests validity proof from RPC
3. On-chain: derive addresses, create compressed accounts
4. If any address exists, tx fails

## Program instruction

### `create_nullifier`

Creates nullifier accounts for provided values.

**Parameters:**
- `data: NullifierInstructionData` - validity proof, tree info, indices
- `nullifiers: Vec<[u8; 32]>` - nullifier values to register

**Behavior:**
- Derives address from `[b"nullifier", nullifier_value]`
- Creates compressed account at derived address
- Fails if address already exists (prevents replay)

## Build and Test

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
