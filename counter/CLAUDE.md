# Counter Program

A counter program that stores state in compressed accounts. Three implementations: Anchor, Native (solana_program), and Pinocchio.

## Summary

- Compressed PDA with address derived from `["counter", signer]` seeds
- `LightAccount` lifecycle: `new_init()` for create, `new_mut()` for update, `new_close()` for delete
- Owner field hashed with Poseidon via `#[hash]` attribute for account hash verification
- Address tree validation enforces `ADDRESS_TREE_V2`
- Closed addresses cannot be reused

## [anchor/README.md](anchor/README.md)

## Source structure

```
counter/
├── anchor/programs/counter/src/lib.rs   # Anchor implementation
├── native/src/lib.rs                    # Native solana_program implementation
└── pinocchio/src/lib.rs                 # Pinocchio implementation
```

## Accounts

### CounterAccount (compressed PDA)

Discriminator: `LightDiscriminator` derive macro generates 8-byte discriminator from struct name hash.

| Field | Type | Hashing | Description |
|-------|------|---------|-------------|
| `owner` | `Pubkey` | Poseidon (`#[hash]`) | Counter owner, included in account hash. |
| `value` | `u64` | None | Counter value, Borsh-serialized only. |

**Address derivation:**

```rust
derive_address(&[b"counter", signer.key().as_ref()], &address_tree_pubkey, &program_id)
```

## Instructions

| Discriminator | Enum Variant | Accounts | Logic |
|---------------|--------------|----------|-------|
| 0 | `CreateCounter` | signer (mut), remaining_accounts | Validates `address_tree_pubkey == ADDRESS_TREE_V2`. Derives address from seeds. Calls `LightAccount::new_init()`, sets owner to signer, value to 0. |
| 1 | `IncrementCounter` | signer (mut), remaining_accounts | Calls `LightAccount::new_mut()` with current state. Executes `checked_add(1)`. Invokes Light System Program. |
| 2 | `DecrementCounter` | signer (mut), remaining_accounts | Calls `LightAccount::new_mut()` with current state. Executes `checked_sub(1)`. Invokes Light System Program. |
| 3 | `ResetCounter` | signer (mut), remaining_accounts | Calls `LightAccount::new_mut()` with current state. Sets value to 0. Invokes Light System Program. |
| 4 | `CloseCounter` | signer (mut), remaining_accounts | Calls `LightAccount::new_close()` (input state only, no output). Address cannot be reused. |

### Instruction data structs

| Struct | Fields |
|--------|--------|
| `CreateCounterInstructionData` | `proof`, `address_tree_info`, `output_state_tree_index` |
| `IncrementCounterInstructionData` | `proof`, `counter_value`, `account_meta` |
| `DecrementCounterInstructionData` | `proof`, `counter_value`, `account_meta` |
| `ResetCounterInstructionData` | `proof`, `counter_value`, `account_meta` |
| `CloseCounterInstructionData` | `proof`, `counter_value`, `account_meta` |

## Security

| Check | Location | Description |
|-------|----------|-------------|
| Address tree validation | `create_counter` | Rejects if `address_tree_pubkey != ADDRESS_TREE_V2`. |
| Overflow protection | `increment_counter` | Uses `checked_add(1)`. |
| Underflow protection | `decrement_counter` | Uses `checked_sub(1)`. |
| Owner binding | All mutations | Owner reconstructed from signer, included in account hash verification. |
| Program ID check | Native/Pinocchio | Validates `program_id == ID` at entry. |

## Errors

| Code | Name | Description |
|------|------|-------------|
| 1 | `Unauthorized` | No authority to perform action. |
| 2 | `Overflow` | Counter increment would overflow u64. |
| 3 | `Underflow` | Counter decrement would underflow below 0. |
