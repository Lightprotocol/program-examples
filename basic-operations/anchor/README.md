# Basic Operations - Anchor Programs

Standalone Anchor programs for compressed accounts.

## Structure

Each operation is an independent Anchor project:

- **create** - Initialize a new compressed account
- **update** - Modify data in an existing compressed account
- **close** - Clear account data while preserving address
- **reinit** - Reinitialize a closed account
- **burn** - Permanently delete a compressed account

Each project contains its own workspace, program, and tests.

## Build

Navigate to a specific project directory and build:

```bash
cd create/  # or update/, close/, reinit/, burn/
anchor build
```

## Test

### Requirements

- light cli version 0.27.1-alpha.2+ (install via `npm install -g @lightprotocol/zk-compression-cli@0.27.1-alpha.2`)
- solana cli version 2.1.16+
- anchor version 0.31.1+
- Node.js and npm

### Running Tests

#### Rust Tests

```bash
cd create/  # or update/, close/, reinit/, burn/
cargo test-sbf
```

#### TypeScript Tests

1. Build the program and sync the program ID:

   ```bash
   cd create/  # or update/, close/, reinit/, burn/
   anchor build && anchor keys sync && anchor build
   ```

2. Start the test validator with the program deployed:

   ```bash
   light test-validator --sbf-program "<PROGRAM_ID>" ./target/deploy/<program_name>.so
   ```

   NOTE: Replace `<PROGRAM_ID>` with the ID from `Anchor.toml` and `<program_name>` with `create`, `update`, `close`, `reinit`, or `burn`.

3. Install dependencies and run tests:

   ```bash
   npm install
   anchor test --skip-local-validator --skip-build --skip-deploy
   ```

The TypeScript tests demonstrate client-side interaction with compressed accounts using `@lightprotocol/stateless.js`.

`light test-validator` spawns the following background processes:

1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:3001`
3. photon indexer `http://127.0.0.1:8784`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.

## Disclaimer

This reference implementation is not audited.

The Light Protocol programs are audited and deployed on Solana devnet and mainnet.