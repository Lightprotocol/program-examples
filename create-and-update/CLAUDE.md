# Create and Update

Demonstrates compressed account lifecycle operations: creation, updates, and atomic multi-account operations in single instructions.

## Summary

- Create compressed accounts with derived addresses using `LightAccount::new_init()`
- Update existing accounts via `LightAccount::new_mut()` which validates input state hash
- Execute atomic multi-account operations in single CPI calls (create+update, update+update, create+create)
- Addresses derived from `[seed, signer.key()]` with program ID and address tree

## [README](README.md)

## Source structure

```
programs/create-and-update/src/
└── lib.rs              # Program entry, instructions, accounts, state structs
```

## Accounts

### Anchor accounts

| Account | Type | Description |
|---------|------|-------------|
| `signer` | `Signer` | Transaction fee payer and account owner. Marked `mut`. |

### Compressed account state

| Struct | Fields | Discriminator |
|--------|--------|---------------|
| `DataAccount` | `owner: Pubkey`, `message: String` | 8-byte hash of "DataAccount" via `LightDiscriminator` |
| `ByteDataAccount` | `owner: Pubkey`, `data: [u8; 31]` | 8-byte hash of "ByteDataAccount" via `LightDiscriminator` |

### PDAs and address derivation

| Address | Seeds | Description |
|---------|-------|-------------|
| First address | `[FIRST_SEED, signer.key()]` | Derived via `derive_address` with program ID and address tree. |
| Second address | `[SECOND_SEED, signer.key()]` | Derived via `derive_address` with program ID and address tree. |

Constants:
- `FIRST_SEED`: `b"first"`
- `SECOND_SEED`: `b"second"`
- `LIGHT_CPI_SIGNER`: Derived via `derive_light_cpi_signer!` macro from program ID

### Instruction data structs

| Struct | Fields | Used by |
|--------|--------|---------|
| `ExistingCompressedAccountIxData` | `account_meta: CompressedAccountMeta`, `message: String`, `update_message: String` | `create_and_update`, `update_two_accounts` |
| `NewCompressedAccountIxData` | `address_tree_info: PackedAddressTreeInfo`, `message: String` | `create_and_update` |

## Instructions

| Discriminator | Instruction | Accounts | Parameters | Logic |
|---------------|-------------|----------|------------|-------|
| sighash("create_compressed_account") | `create_compressed_account` | `GenericAnchorAccounts` + remaining accounts | `proof`, `address_tree_info`, `output_state_tree_index`, `message` | Validates address tree is ADDRESS_TREE_V2. Derives address from FIRST_SEED + signer. Creates `DataAccount` via `LightAccount::new_init()`. Invokes Light System Program CPI. |
| sighash("create_and_update") | `create_and_update` | `GenericAnchorAccounts` + remaining accounts | `proof`, `existing_account`, `new_account` | Creates new `DataAccount` at SECOND_SEED. Updates existing account via `LightAccount::new_mut()` (validates current state hash). Single CPI with both operations. |
| sighash("update_two_accounts") | `update_two_accounts` | `GenericAnchorAccounts` + remaining accounts | `proof`, `first_account`, `second_account` | Updates two existing `DataAccount` messages atomically via `LightAccount::new_mut()`. Single CPI call. |
| sighash("create_two_accounts") | `create_two_accounts` | `GenericAnchorAccounts` + remaining accounts | `proof`, `address_tree_info`, `output_state_tree_index`, `byte_data`, `message` | Creates `ByteDataAccount` at FIRST_SEED and `DataAccount` at SECOND_SEED in single CPI. |

## Security

| Check | Location | Description |
|-------|----------|-------------|
| Address tree validation | `lib.rs:49-52`, `lib.rs:97-100`, `lib.rs:218-221` | Verifies `address_tree_pubkey` matches `ADDRESS_TREE_V2`. |
| Signer authorization | Anchor `#[account(mut)]` | Signer must sign transaction and pay fees. |
| CPI signer derivation | `lib.rs:17-18` | `LIGHT_CPI_SIGNER` derived from program ID via `derive_light_cpi_signer!` macro. |

## Errors

| Error | Source | Cause |
|-------|--------|-------|
| `AccountNotEnoughKeys` | `ErrorCode::AccountNotEnoughKeys` | Address tree pubkey cannot be retrieved from remaining accounts. |
| `InvalidAccountData` | `ProgramError::InvalidAccountData` | Address tree pubkey does not match `ADDRESS_TREE_V2`. |
