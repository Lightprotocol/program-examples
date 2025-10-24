# Basic Operations - Native Rust Programs

Native Solana programs with basic compressed account operations.

## Programs

- **create** - Initialize a new compressed account
- **update** - Modify data in an existing compressed account
- **close** - Close a compressed account and reclaim rent
- **reinit** - Reinitialize a previously closed compressed account
- **burn** - Permanently destroy a compressed account (cannot be reinitialized)

## Build

Build all programs in the workspace:

```bash
cargo build-sbf
```

The compiled `.so` files will be in `target/deploy/`

## Test

### Requirements

- light cli version 0.24.0+ (install via `npm i -g @lightprotocol/zk-compression-cli`)
- solana cli version 2.1.16+
- Node.js and npm

### Running Tests

#### Rust Tests

```bash
cargo test-sbf
```

#### TypeScript Tests

1. Build the programs:

   ```bash
   cargo build-sbf
   ```

2. Start the test validator with deployed programs:

   ```bash
   light test-validator \
     --sbf-program "<PROGRAM_ID_CREATE>" ./target/deploy/create.so \
     --sbf-program "<PROGRAM_ID_UPDATE>" ./target/deploy/update.so \
     --sbf-program "<PROGRAM_ID_CLOSE>" ./target/deploy/close.so \
     --sbf-program "<PROGRAM_ID_REINIT>" ./target/deploy/reinit.so \
     --sbf-program "<PROGRAM_ID_BURN>" ./target/deploy/burn.so
   ```

   NOTE: Replace program IDs with those defined in each program's `lib.rs` (`pub const ID`).

3. Install dependencies and run tests:

   ```bash
   npm install

   npm test
   ```

The TypeScript tests demonstrate client-side interaction with compressed accounts using `@lightprotocol/stateless.js` and `@lightprotocol/zk-compression-cli`.

`light test-validator` spawns the following background processes:

1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:3001`
3. photon indexer `http://127.0.0.1:8784`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.

## Disclaimer

This reference implementation is not audited.

The Light Protocol programs are audited and deployed on Solana devnet and mainnet.
