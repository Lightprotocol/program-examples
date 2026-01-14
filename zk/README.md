# ZK Examples

Building a private Solana program requires a Merkle tree to store state, a way to track nullifiers, and an indexer to serve Merkle proofs.

You can use Light to:
- Track and store nullifiers rent-free in indexed address Merkle trees
- Store state rent-free in indexed state Merkle trees as compressed accounts

[Learn more in the documentation](https://www.zkcompression.com/zk/overview)

## Examples

- **[zk-id](./zk-id)** - Identity verification using Groth16 proofs. Issuers create credentials; users prove ownership without revealing the credential.
- **[nullifier](./nullifier)** - Simple Program to Create Nullifiers. Requires no custom circuit.

## Building and Testing

A Makefile is provided for building, deploying, and testing all examples:

```bash
# Build all programs
make build

# Deploy all programs to local validator
make deploy

# Run Rust tests (cargo test-sbf)
make test-rust

# Run TypeScript tests (deploys programs first)
make test-ts

# Build and run all tests
make all

# Individual examples
make zk-nullifier
make zk-id

# Show all available commands
make help
```

## Light Protocol V2 API

These examples use Light Protocol SDK v0.17+ with the V2 accounts layout:

- `light_sdk::cpi::v2::CpiAccounts` - V2 CPI accounts builder
- `light_sdk::cpi::v2::LightSystemProgramCpi` - V2 system program CPI
- `PackedAddressTreeInfo::into_new_address_params_assigned_packed()` - V2 address parameters

The V2 layout requires a `system_accounts_offset` parameter to locate system accounts in remaining accounts.
