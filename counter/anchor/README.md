# Compressed Counter Program (Anchor)

A counter program built with the Anchor framework. Includes instructions to create a zk-compressed PDA account, increment, decrement, reset the counter value, and close the account.

## Build

```bash
anchor build
```

## Test

### Requirements

- light cli version 0.24.0+ (install via npm `i -g @lightprotocol/zk-compression-cli`)
- solana cli version 2.1.16+
- anchor version 0.31.1+
- Node.js and npm

### Running Tests

#### Rust Tests

```bash
cargo test-sbf
```

#### TypeScript Tests

1. Build the program and sync the program ID:

   ```bash
   anchor build && anchor keys sync && anchor build
   ```

2. Start the test validator

   ```bash
   light test-validator --sbf-program "GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX" ./target/deploy/counter.so
   ```
   
NOTE: Replace the program ID above with the one generated in your `Anchor.toml` file.

3. Install dependencies and run tests:

   ```bash
   npm install

   anchor test --skip-local-validator --skip-build --skip-deploy
   ```

The TypeScript tests demonstrate client-side interaction with compressed accounts using `@lightprotocol/stateless.js` and `@lightprotocol/zk-compression-cli`.

`$ light test-validator` spawns the following background processes:

1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:8784`
3. photon indexer `http://127.0.0.1:3001`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.

## Disclaimer

This reference implementation is not audited.

The Light Protocol programs are audited and deployed on Solana devnet and mainnet.
