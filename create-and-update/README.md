# Create and Update Example

This example demonstrates the basic operations of compressed accounts using Light Protocol. It shows how to create compressed accounts and how to atomically create and update compressed accounts in a single instruction.

## Instructions

### 1. `create_compressed_account`
Creates a new compressed account with initial data (owner and message).

### 2. `create_and_update`
Demonstrates atomic operations in a single instruction:
- Creates a new compressed account with a "second" seed
- Updates an existing compressed account (created with "first" seed)
- Uses a single validity proof to prove inclusion of the existing account and create the new address

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

## Key Concepts Demonstrated

- **Compressed Account Creation**: Using `LightAccount::new_init()` to create new compressed accounts
- **Compressed Account Updates**: Using `LightAccount::new_mut()` to update existing compressed accounts
- **Address Derivation**: Using deterministic seeds (`FIRST_SEED`, `SECOND_SEED`) for address generation
- **Atomic Operations**: Performing multiple compressed account operations in a single instruction
- **Authorization**: Verifying ownership before allowing updates
- **Single Validity Proof**: Using one proof to handle both input (existing account) and output (new account) operations