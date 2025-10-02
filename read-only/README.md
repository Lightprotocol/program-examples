# Read-only example

This program demonstrates how to create and then read a compressed account on-chain.

## Instructions

### 1. `create_compressed_account`
Creates a new compressed account with initial data (owner and message).

### 2. `read`
Demonstrates reading an existing compressed account on-chain:
- Uses a single validity proof to prove inclusion of the existing account

## Data Structure

```rust
pub struct DataAccount {
    #[hash]
    pub owner: Pubkey,
    #[hash]
    pub message: String,
}
```

## Build and Test

```bash
# Build the program
cargo build-sbf

# Run tests
cargo test-sbf
```
