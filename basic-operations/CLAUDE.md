# Basic Operations Examples

Example programs showing all basic operations for compressed accounts.

## Summary

- Demonstrates create, update, close, reinit, and burn operations
- Anchor uses Poseidon hashing (`light_sdk::account::LightAccount`); Native uses SHA-256 (`light_sdk::account::sha::LightAccount`)
- All programs derive addresses from seeds `[b"message", signer.key.as_ref()]` + `ADDRESS_TREE_V2` + program ID
- Close preserves address for reinitialization; burn permanently deletes address

## [Anchor README](anchor/README.md) | [Native README](native/README.md)

## Source structure

```
basic-operations/
├── anchor/                          # Anchor framework (Poseidon hashing)
│   ├── create/programs/create/src/lib.rs
│   ├── update/programs/update/src/lib.rs
│   ├── close/programs/close/src/lib.rs
│   ├── reinit/programs/reinit/src/lib.rs
│   └── burn/programs/burn/src/lib.rs
└── native/                          # Native Solana (SHA-256 hashing)
    └── programs/
        ├── create/src/lib.rs
        ├── update/src/lib.rs
        ├── close/src/lib.rs
        ├── reinit/src/lib.rs
        └── burn/src/lib.rs
```

## Accounts

### MyCompressedAccount

Shared account structure across all examples. Derives `LightDiscriminator` for compressed account type identification.

```rust
#[derive(LightDiscriminator)]
pub struct MyCompressedAccount {
    pub owner: Pubkey,    // Account owner
    pub message: String,  // User-defined message
}
```

## Instructions

Native programs use `InstructionType` enum discriminators (first byte of instruction data).

### create_account (discriminator: 0)

| Parameter | Type | Description |
|-----------|------|-------------|
| `proof` | `ValidityProof` | ZK proof for address non-existence |
| `address_tree_info` | `PackedAddressTreeInfo` | Address tree metadata |
| `output_state_tree_index` | `u8` | Target state tree index |
| `message` | `String` | Initial message content |

Calls `LightAccount::new_init()` with derived address, invokes `LightSystemProgramCpi` with `with_new_addresses()`.

### update_account (discriminator: 1)

| Parameter | Type | Description |
|-----------|------|-------------|
| `proof` | `ValidityProof` | ZK proof for account existence |
| `account_meta` | `CompressedAccountMeta` | Current account metadata |
| `current_account` / `current_message` | varies | Current account state for verification |
| `new_message` | `String` | Updated message content |

Calls `LightAccount::new_mut()` with current state, modifies message, invokes CPI.

### close_account (discriminator: 1)

| Parameter | Type | Description |
|-----------|------|-------------|
| `proof` | `ValidityProof` | ZK proof for account existence |
| `account_meta` | `CompressedAccountMeta` | Current account metadata |
| `current_message` | `String` | Current message for verification |

Calls `LightAccount::new_close()` - clears data but preserves address for reinitialization.

### reinit_account (discriminator: 2)

| Parameter | Type | Description |
|-----------|------|-------------|
| `proof` | `ValidityProof` | ZK proof for empty account at address |
| `account_meta` | `CompressedAccountMeta` | Account metadata |

Calls `LightAccount::new_empty()` to reinitialize previously closed account.

### burn_account (discriminator: 1)

| Parameter | Type | Description |
|-----------|------|-------------|
| `proof` | `ValidityProof` | ZK proof for account existence |
| `account_meta` | `CompressedAccountMetaBurn` | Account metadata (burn-specific) |
| `current_message` / `current_account` | varies | Current state for verification |

Calls `LightAccount::new_burn()` - permanently deletes account. Address cannot be reused.

## Security

- Address tree validation: Checks `address_tree_pubkey.to_bytes() == ADDRESS_TREE_V2`
- Program ID verification (native only): `program_id != &ID` returns `IncorrectProgramId`
- Signer required: First account must be mutable signer
- State verification: Close/update/burn require current state to match on-chain data

## Errors

| Error | Source | Condition |
|-------|--------|-----------|
| `ProgramError::IncorrectProgramId` | Native entrypoint | Program ID mismatch |
| `ProgramError::InvalidInstructionData` | Entrypoint | Empty or malformed instruction data |
| `ProgramError::NotEnoughAccountKeys` | All | Missing required accounts |
| `ProgramError::InvalidAccountData` | All | Invalid address tree |
| `LightSdkError::Borsh` | Native | Instruction data deserialization failure |
