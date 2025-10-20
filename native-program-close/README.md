# Close Compressed Account Program (Native Rust)

A Native Solana program to close a zk-compressed account. The account can be reinitialized after closing.

## Build

```bash
cargo build-sbf
```

The compiled `.so` file will be in `target/deploy/native_program_close.so`

## Test

### Requirements

- light cli version 0.24.0+ (install via npm `i -g @lightprotocol/zk-compression-cli`)
- solana cli version 2.1.16+
- Node.js and npm

### Running Tests

#### Rust Tests

```bash
cargo test-sbf
```

#### TypeScript Tests

1. Build the program:

   ```bash
   cargo build-sbf
   ```

2. Start the test validator

   ```bash
   light test-validator --sbf-program "rent4o4eAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPq" ./target/deploy/native_program_close.so
   ```

NOTE: Replace the program ID above with the one defined in `program/src/lib.rs` (`pub const ID`).

3. Install dependencies and run tests:

   ```bash
   npm install

   npm test
   ```

The TypeScript tests demonstrate client-side interaction with compressed accounts using `@lightprotocol/stateless.js` and `@lightprotocol/zk-compression-cli`.

`$ light test-validator` spawns the following background processes:

1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:3001`
3. photon indexer `http://127.0.0.1:8784`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.

## Disclaimer

This reference implementation is not audited.

The Light Protocol programs are audited and deployed on Solana devnet and mainnet.
