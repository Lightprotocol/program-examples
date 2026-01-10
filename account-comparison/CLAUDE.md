# Account Comparison

Side-by-side implementation of Solana accounts vs compressed accounts. Shows equivalent create and update operations for both account types using identical data structures.

## Summary

- Demonstrates equivalent PDA and compressed account patterns with identical seeds `["account", user.key()]`
- Compressed accounts use `LightAccount::new_init` for creation and `LightAccount::new_mut` for updates
- Updates require client to pass existing state because on-chain storage is a Poseidon hash
- All fields marked `#[hash]` are included in Poseidon hash computation

## Source Structure

```
programs/account-comparison/src/
  lib.rs              # Program entrypoint, instructions, account structs
programs/account-comparison/tests/
  test_solana_account.rs      # LiteSVM tests for standard accounts
  test_compressed_account.rs  # Light Protocol tests for compressed accounts
```

## Accounts

### AccountData (Solana PDA)

| Field | Type | Size |
|-------|------|------|
| discriminator | `[u8; 8]` | 8 bytes |
| user | `Pubkey` | 32 bytes |
| name | `String` | 4 + name_len (max 60 chars) |
| data | `[u8; 128]` | 128 bytes |

- **Seeds**: `["account", user.key()]`
- **Discriminator**: 8 bytes, SHA256("account:AccountData")[0..8]
- **Space**: 232 bytes. String uses Borsh serialization (4-byte length prefix + variable content).

### CompressedAccountData (LightAccount)

```rust
#[derive(LightDiscriminator, LightHasher)]
pub struct CompressedAccountData {
    #[hash] pub user: Pubkey,
    #[hash] pub name: String,
    #[hash] pub data: [u8; 128],
}
```

- **Address seeds**: `["account", user.key()]`
- **Discriminator**: `LightDiscriminator` derive generates from struct name
- **Hashing**: Poseidon hash of all `#[hash]` fields (user, name, data)

### CPI Signer

```rust
const CPI_SIGNER: CpiSigner = derive_light_cpi_signer!("FYX4GmKJYzSiycc7XZKf12NGXNE9siSx1cJubYJniHcv");
```

## Instructions

| # | Instruction | Accounts | Parameters | Logic |
|---|-------------|----------|------------|-------|
| 0 | `create_account` | `user` (signer, mut), `account` (init PDA), `system_program` | `name: String` | Initializes PDA with seeds `["account", user]`, sets `data = [1u8; 128]` |
| 1 | `update_data` | `user` (signer, mut), `account` (mut, has_one = user) | `data: [u8; 128]` | Overwrites account.data |
| 2 | `create_compressed_account` | `user` (signer, mut) + remaining_accounts | `name`, `proof`, `address_tree_info`, `output_tree_index` | Validates ADDRESS_TREE_V2, derives address, calls `LightAccount::new_init`, invokes light-system-program |
| 3 | `update_compressed_account` | `user` (signer, mut) + remaining_accounts | `new_data`, `existing_data`, `name`, `proof`, `account_meta` | Reconstructs state via `LightAccount::new_mut`, verifies user ownership, invokes light-system-program |

## Security

| Check | Location | Description |
|-------|----------|-------------|
| Address tree validation | `create_compressed_account` | Verifies `address_tree_pubkey.to_bytes() == ADDRESS_TREE_V2` |
| Owner verification | `update_compressed_account` | Asserts `compressed_account.user == ctx.accounts.user.key()` |
| PDA constraint | `update_data` | Anchor `has_one = user` constraint |
| Signer requirement | All instructions | User must sign transaction |

## Errors

| Error | Message |
|-------|---------|
| `CustomError::Unauthorized` | "No authority to perform this action" |
| `ProgramError::InvalidAccountData` | Returned when address tree validation fails |
