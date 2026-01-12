# Compressed Accounts Program Examples

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/Lightprotocol/program-examples) to query the program examples in natural language and for help with debugging.

## Examples

### Airdrop Claim Reference Implementations

* **Basic**: [**simple-claim**](./airdrop-implementations/simple-claim) - Distributes compressed tokens that get decompressed to SPL on claim with cliff.
* **Advanced**: [**merkle-distributor**](./airdrop-implementations/distributor) - Distributes SPL tokens, uses compressed PDAs to track claims with linear vesting, partial claims and clawback. Based on Jito Merkle distributor and optimized with rent-free PDAs.

For simple client side distribution visit [this example](https://github.com/Lightprotocol/example-token-distribution).

### Basic Operations

- **[create-nullifier](./basic-operations/anchor/create-nullifier)** - Basic Anchor example to create nullifiers for payments.
- **create** - Initialize a new compressed account
  - [Anchor](./basic-operations/anchor/create) | [Native](./basic-operations/native/programs/create)
- **update** - Modify data in an existing compressed account
  - [Anchor](./basic-operations/anchor/update) | [Native](./basic-operations/native/programs/update)
- **close** - Clear account data and preserve its address
  - [Anchor](./basic-operations/anchor/close) | [Native](./basic-operations/native/programs/close)
- **reinit** - Reinitialize a closed account with the same address
  - [Anchor](./basic-operations/anchor/reinit) | [Native](./basic-operations/native/programs/reinit)
- **burn** - Permanently delete a compressed account
  - [Anchor](./basic-operations/anchor/burn) | [Native](./basic-operations/native/programs/burn)

### Counter Program

Full compressed account lifecycle (create, increment, decrement, reset, close):

- **[counter/anchor](./counter/anchor/)** - Anchor program with Rust and TypeScript tests
- **[counter/native](./counter/native/)** - Native Solana program with light-sdk and Rust tests.
- **[counter/pinocchio](./counter/pinocchio/)** - Pinocchio program with light-sdk-pinocchio and Rust tests.


### Create-and-update Program

- **[create-and-update](./create-and-update/)** - Create a new compressed account and update an existing compressed account with a single validity proof in one instruction.

### Create-and-read Program

- **[read-only](./read-only)** - Create a new compressed account and read it onchain.


### Compare Program with Solana vs Compressed Accounts

- **[account-comparison](./account-comparison/)** - Compare compressed vs regular Solana accounts.

### zk-id Program

- **[zk-id](./zk-id)** - A minimal zk id Solana program that uses zero-knowledge proofs for identity verification with compressed accounts.

  
## Light Protocol dependencies

### Rust Crates

- `light-sdk` - Core SDK for compressed accounts in native and anchor programs
- `light-sdk-pinocchio` Core SDK for compressed accounts in pinocchio programs
- `light-hasher` - Hashing utilities for ZK compression
- `light-client` - RPC client and indexer for interacting with compressed accounts
- `light-program-test` - Testing utilities for compressed programs.

### TypeScript/JavaScript Packages

- `@lightprotocol/stateless.js@0.22.1-alpha.1` - Client library for interacting with compressed accounts
- `@lightprotocol/zk-compression-cli@0.27.1-alpha.2` - Command-line tools for ZK compression development

## Prerequisites

Required versions:

- **Rust**: 1.90.0 or later
- **Solana CLI**: 2.3.11
- **Anchor CLI**: 0.31.1
- **Zk compression CLI**: 0.27.1-alpha.2 or later
- **Node.js**: 23.5.0 or later

Install the Light CLI:

```bash
$ npm -g i @lightprotocol/zk-compression-cli@0.27.1-alpha.2
```

Install Solana CLI:

```bash
sh -c "$(curl -sSfL https://release.solana.com/v2.3.11/install)"
```

Install Anchor CLI:

```bash
cargo install --git https://github.com/coral-xyz/anchor avm --force
avm install latest
avm use 0.31.1
```

## Getting Started with your own Program

1. install the light cli

```bash
$ npm -g i @lightprotocol/zk-compression-cli@0.27.1-alpha.2
```

2. instantiate a template Solana program with compressed accounts

```bash
$ light init <project-name>
```
