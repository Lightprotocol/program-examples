# Compressed Accounts Examples

Examples for building with ZK compression by Light Protocol.

## Prerequisites

Install the Light CLI:
```bash
$ npm -g i @lightprotocol/zk-compression-cli
```

## Light Protocol Packages Used

### Rust Packages
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
A counter program demonstrating compressed account operations (create, increment, decrement, reset, close) implemented in three different ways:

- **[counter/anchor](./counter/anchor/)** - Anchor framework implementation of a compressed counter account with Rust and TypeScript tests
- **[counter/native](./counter/native/)** - Native Solana program implementation of a compressed counter account
- **[counter/pinocchio](./counter/pinocchio/)** - Pinocchio SDK implementation of a compressed counter account

### Other Examples

- **[account-comparison](./account-comparison/)** - Compare compressed vs regular Solana accounts


## Getting Started

1. install the light cli
```bash
$ npm -g i @lightprotocol/zk-compression-cli
```
2. instantiate a template Solana program with compressed accounts
```bash
$ light init <project-name>
```
