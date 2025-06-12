# Compressed Accounts Examples

Examples for building with ZK compression by Light Protocol.

## Prerequisites

Install the Light CLI:
```bash
$ npm -g i @lightprotocol/zk-compression-cli
```

## Light Protocol Libraries Used

### Rust Crates
- `light-sdk` - Core SDK for compressed accounts in native and anchor programs
- `light-sdk-pinocchio` Core SDK for compressed accounts in pinocchio programs
- `light-hasher` - Hashing utilities for ZK compression
- `light-client` - RPC client and indexer for interacting with compressed accounts
- `light-program-test` - Testing utilities for compressed programs.


### TypeScript/JavaScript Packages
- `@lightprotocol/stateless.js` - Client library for interacting with compressed accounts
- `@lightprotocol/zk-compression-cli` - Command-line tools for ZK compression development

## Examples

### Counter Program
The counter program implements a compressed account lifecycle (create, increment, decrement, reset, close) in three different ways:

- **[counter/anchor](./counter/anchor/)** - Anchor framework implementation with Rust and TypeScript tests
- **[counter/native](./counter/native/)** - Native Solana program implementation using Light SDK
- **[counter/pinocchio](./counter/pinocchio/)** - Pinocchio SDK implementation for streamlined development

### Create and Update Program

- **[create-and-update](./create-and-update/)** - Create a new compressed account and update an existing compressed account with a single validity proof in one instruction.

### Solana vs compressed accounts comparison Program

- **[account-comparison](./account-comparison/)** - Compare compressed vs regular Solana accounts.


## Getting Started

1. install the light cli
```bash
$ npm -g i @lightprotocol/zk-compression-cli
```
2. instantiate a template Solana program with compressed accounts
```bash
$ light init <project-name>
```
