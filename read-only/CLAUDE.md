# Read-Only Example Program

Creates and reads compressed accounts. Verifies on-chain state without modification.

## Summary

- Creates compressed accounts with derived addresses and validates them read-only via Light System Program CPI
- Read-only validation reconstructs account data client-side, program verifies data hash matches proven state
- Requires v2 address trees (`ADDRESS_TREE_V2`)
- Uses `LightAccount::new_read_only()` for non-mutating access

## [README](README.md)

## Source structure

```
src/
└── lib.rs          # Program entry, instructions, account definitions
tests/
└── test.rs         # Integration tests for create and read operations
```

## Accounts

### Anchor accounts

| Account | Type | Description |
|---------|------|-------------|
| `signer` | `Signer` | Transaction fee payer and account owner. Mutable. |

### DataAccount (compressed)

8-byte discriminator derived via `LightDiscriminator`. Stored in separate discriminator field, not in data bytes.

```rust
pub struct DataAccount {
    pub owner: Pubkey,    // Account owner's public key
    pub message: String,  // User-defined message
}
```

**Address derivation:** `["first", signer.key()]` via `derive_address()` with address tree and program ID.

### ExistingCompressedAccountIxData

Instruction data struct for `read`. Contains `CompressedAccountMetaReadOnly` (tree indices, leaf info) and current `message` for hash verification.

## Instructions

| Instruction | Accounts | Parameters | Logic |
|-------------|----------|------------|-------|
| `create_compressed_account` | `signer` (mut) + remaining accounts | `proof: ValidityProof`, `address_tree_info: PackedAddressTreeInfo`, `output_state_tree_index: u8`, `message: String` | Validates address tree is v2. Derives address from seeds. Initializes `LightAccount<DataAccount>` with owner and message. Invokes Light System Program CPI with `with_new_addresses()`. |
| `read` | `signer` (mut) + remaining accounts | `proof: ValidityProof`, `existing_account: ExistingCompressedAccountIxData` | Reconstructs `DataAccount` from instruction data. Creates `LightAccount::new_read_only()` which computes data hash from provided fields. Light System Program CPI verifies hash matches proven Merkle leaf. |

## Security

| Check | Location | Description |
|-------|----------|-------------|
| Address tree validation | `create_compressed_account` | Verifies `address_tree_pubkey` matches `ADDRESS_TREE_V2` constant |
| Signer verification | Both instructions | `signer` account is `Signer` type, Anchor validates signature |
| Owner assignment | `create_compressed_account` | Sets `data_account.owner = signer.key()` |
| Read-only data hash verification | `read` | Light System Program verifies reconstructed data hashes match proven state |

## Errors

| Error | Source | Cause |
|-------|--------|-------|
| `AccountNotEnoughKeys` | `create_compressed_account` | Address tree pubkey lookup failed from `CpiAccounts` |
| `InvalidAccountData` | `create_compressed_account` | Address tree pubkey does not match `ADDRESS_TREE_V2` |
