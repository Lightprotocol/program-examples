# c-Token Examples

Program and Rust Client examples for c-Tokens using the `light_ctoken_sdk`.

## Prerequisites

Requirements
- light cli version 0.27.1-alpha.2+
- solana cli version 2.1.16+
- Node.js and npm


Install dependencies:

```bash
# Install Light CLI
npm -g i @lightprotocol/zk-compression-cli
```

Start the local test validator with Light programs:

```bash
light test-validator
```

# Build 

```bash
cd create/  # or update/, close/, reinit/, burn/
cargo build-sbf
```

The compiled `.so` files will be in `target/deploy/`.


# Test
## Rust Tests

```bash
cd create/  # or update/, close/, reinit/, burn/
cargo test-sbf

```

# Notes

`light test-validator` spawns the following background processes:

1. solana test validator `http://127.0.0.1:8899`
2. prover server `http://127.0.0.1:3001`
3. photon indexer `http://127.0.0.1:8784`

You can kill these background processes with `lsof -i:<port>` and `kill <pid>`.

## Disclaimer

This reference implementation is not audited.

The Light Protocol programs are audited and deployed on Solana devnet and mainnet.