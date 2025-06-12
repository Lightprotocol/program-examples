# Compressed Counter Program (Anchor)

This is a counter program implemented using the Anchor framework with instructions to create a compressed account, increment, decrement, reset the counter value, and close the account. It demonstrates the complete lifecycle of compressed accounts using Light Protocol's ZK compression.

## Build

```bash
anchor build
```

## Test

### Requirements
- light cli version 0.24.0+
- solana cli version 2.1.16+
- anchor version 0.31.1+
- Node.js and npm

### Running Tests

#### Rust Tests
```bash
anchor test
```

#### TypeScript Tests
1. Start the test validator with skip-prover flag:
   ```bash
   light test-validator --skip-prover --sbf-program GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX ./target/deploy/counter.so
   ```

2. Install dependencies and run tests:
   ```bash
   npm install
   npm test
   ```

The TypeScript tests demonstrate client-side interaction with compressed accounts using `@lightprotocol/stateless.js`.

`$ light test-validator` spawns the following background processes:
1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:8784`
3. photon indexer `http://127.0.0.1:3001`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.


## Disclaimer

Light Protocol programs are audited and deployed on Solana devnet and mainnet.
