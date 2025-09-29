# Light SDK v2 Migration Guide

This guide helps you migrate Light Protocol examples from the old SDK API to the new v2 SDK API.

## Quick Migration Checklist

- [ ] Update Cargo.toml dependencies to use specific commit hash (not `branch = "main"`)
- [ ] Run `cargo update` after changing dependencies
- [ ] Change imports from `address::v1` to `address::v2`
- [ ] Move CPI trait imports inside `#[program]` module
- [ ] Replace `CpiAccounts` with `CpiAccountsV2`
- [ ] Replace `CpiInputs` pattern with `LightSystemProgramCpi::new_cpi()`
- [ ] Add `.mode_v2()` to all instruction builders
- [ ] Update address params: `into_new_address_params_packed()` → `into_new_address_params_assigned_packed(seed.into(), Some(index))`
- [ ] Update test configuration: `ProgramTestConfig::new()` → `ProgramTestConfig::new_v2()`
- [ ] Update test methods: `add_system_accounts()` → `add_system_accounts_v2()`
- [ ] Update test methods: `get_address_tree_v1()` → `get_address_tree_v2()`
- [ ] Handle `Option<CompressedAccount>` in test results with additional `.unwrap()`
- [ ] Fix validity proof handling in tests (remove double `.unwrap()`)

## Overview of Changes

The Light SDK v2 introduces a cleaner, more intuitive API with the following key changes:
- Unified CPI account management with `CpiAccountsV2`
- Standardized instruction builder pattern with `LightSystemProgramCpi`
- Simplified error handling and method chaining
- Direct account passing without intermediate conversions

## Import Changes

### Old Imports
```rust
use light_sdk::cpi::{InvokeLightSystemProgram, WithLightAccount};
use light_sdk::{
    account::LightAccount,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    // ...
};
use light_sdk_types::{
    cpi_context_write::CpiContextWriteAccounts,
    CpiAccountsConfig,
    CpiAccountsSmall,
};
```

### New Imports

**Important**: Some imports must be placed inside your `#[program]` module:

```rust
// File-level imports
use light_sdk::address::v2::derive_address; // Note: v2, not v1
use light_sdk::{
    account::LightAccount,
    cpi::{CpiAccountsV2, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher,
};

#[program]
pub mod your_program {
    use super::*;
    // These imports MUST be inside the program module
    use light_sdk::cpi::{LightSystemProgramCpi, LightCpiInstruction, InvokeLightSystemProgram};
}
```

## Migration Patterns

### 1. CPI Account Setup

#### Before
```rust
let cpi_accounts = CpiAccounts::new(
    signer.as_ref(),
    remaining_accounts,
    cpi_signer,
);

// Or for small accounts
let light_cpi_accounts = CpiAccountsSmall::new_with_config(
    signer.as_ref(),
    remaining_accounts,
    config,
);
```

#### After
```rust
// Standard setup
let cpi_accounts = CpiAccountsV2::new(
    signer.as_ref(),
    remaining_accounts,
    cpi_signer,
);

// With configuration
let light_cpi_accounts = CpiAccountsV2::new_with_config(
    &fee_payer,
    remaining_accounts,
    CpiAccountsConfig {
        cpi_context: true,
        cpi_signer: cpi_signer,
        sol_compression_recipient: false,
        sol_pool_pda: false,
    },
);
```

### 2. Standard Compressed Account Operations

#### Before
```rust
let cpi = CpiInputs::new_with_address(
    proof,
    vec![counter.to_account_info().map_err(ProgramError::from)?],
    vec![new_address_params],
);
cpi.invoke_light_system_program(cpi_accounts)
    .map_err(ProgramError::from)?;
```

#### After
```rust
// ✅ Use new_cpi() constructor, NOT new()
LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
    .mode_v2()  // Required for v2
    .with_light_account(counter)?
    .with_new_addresses(&[new_address_params])
    .invoke(cpi_accounts)?;
```

**Note**: Always use `new_cpi()` constructor. The `new()` constructor exists but requires manual parameter conversion.

### 3. Address Parameter Creation

#### Before
```rust
let new_address_params = address_tree_info.into_new_address_params_packed(seed);
```

#### After
```rust
let new_address_params =
    address_tree_info.into_new_address_params_assigned_packed(seed.into(), Some(0));
```

### 4. CPI Context Operations (Special Cases)

For CPI context write operations, still use `InstructionDataInvokeCpiWithReadOnly`:

#### Before
```rust
InstructionDataInvokeCpiWithReadOnly::new(
    cpi_signer.program_id.into(),
    cpi_signer.bump,
    None,
)
.mode_v2()
.with_input_compressed_accounts(vec![in_account])
.with_output_compressed_accounts(vec![out_account])
.invoke_write_to_cpi_context_first(&cpi_context_accounts.to_account_infos())?;
```

#### After
```rust
InstructionDataInvokeCpiWithReadOnly::new_cpi(LIGHT_CPI_SIGNER, None.into())
    .mode_v2()
    .with_input_compressed_accounts(&[in_account])
    .with_output_compressed_accounts(&[out_account])
    .invoke_write_to_cpi_context_first(cpi_context_accounts)?;
```

### 5. Execute CPI Context

#### Before
```rust
InstructionDataInvokeCpiWithReadOnly::new(
    cpi_signer.program_id.into(),
    cpi_signer.bump,
    proof.into(),
)
.mode_v2()
.with_light_account(counter)
.map_err(ProgramError::from)?
.invoke_execute_cpi_context(&light_cpi_accounts.to_account_infos())?;
```

#### After
```rust
LightSystemProgramCpi::new(
    LIGHT_CPI_SIGNER.program_id.into(),
    LIGHT_CPI_SIGNER.bump,
    proof.into(),
)
.mode_v2()
.with_light_account(counter)?
.invoke_execute_cpi_context(light_cpi_accounts)?;
```

### 6. Error Handling Simplification

#### Before
```rust
let counter = LightAccount::<'_, CounterAccount>::new_mut(
    &program_id,
    &account_meta,
    data,
)
.map_err(ProgramError::from)?;

counter.to_account_info().map_err(ProgramError::from)?
```

#### After
```rust
let counter = LightAccount::<'_, CounterAccount>::new_mut(
    &program_id,
    &account_meta,
    data,
)?;  // Direct ? operator

counter.to_account_info()?  // No explicit map_err needed
```

### 7. Account Info Passing

#### Before
```rust
// Required .to_account_infos() conversion
invoke_function(&cpi_accounts.to_account_infos())?;
```

#### After
```rust
// Direct passing of CpiAccountsV2
invoke_function(cpi_accounts)?;
```

## Decision Tree for Instruction Builders

```
Need to build Light Protocol instruction?
    │
    ├─ Standard operations (create, update, transfer)?
    │   └─> Use `LightSystemProgramCpi`
    │
    └─ Special CPI context operations?
        ├─ Write to CPI context first?
        │   └─> Use `InstructionDataInvokeCpiWithReadOnly`
        │       with `.invoke_write_to_cpi_context_first()`
        │
        └─ Other special cases
            └─> Use `InstructionDataInvokeCpiWithReadOnly`
```

## Client-Side Changes

### System Accounts v2

The v2 SDK requires using v2 system accounts in your client code:

#### Before (v1 accounts)
```rust
// Client transaction building
let transaction = Transaction::new(
    &instructions,
    &payer.pubkey(),
    &blockhash,
    &accounts,
);
```

#### After (v2 accounts)
```rust
// Client transaction building with v2 accounts
use light_sdk::instruction::PackedAccounts;
use light_sdk_types::SystemAccountMetaConfig;

// Create packed accounts and add v2 system accounts
let mut remaining_accounts = PackedAccounts::default();
let config = SystemAccountMetaConfig::new(program_id);
remaining_accounts.add_system_accounts_v2(config)?;

// Use these remaining accounts in your instruction
let instruction = Instruction {
    program_id,
    accounts: vec![
        // Your regular accounts
        AccountMeta::new(signer.pubkey(), true),
        // ... other accounts
    ],
    data: instruction_data,
};

// Convert packed accounts to account metas for the instruction
let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

// Create the instruction with remaining accounts
let instruction = Instruction {
    program_id,
    accounts: [
        accounts.to_account_metas(Some(true)),
        remaining_accounts_metas,
    ]
    .concat(),
    data: instruction_data.data(),
};

// Send the transaction
let signature = rpc.process_transaction(&[instruction]).await?;
```

#### In Tests (Complete Pattern)
```rust
// In test code, add v2 system accounts to PackedAccounts
let mut remaining_accounts = PackedAccounts::default();
let config = SystemAccountMetaConfig::new(counter::ID);
remaining_accounts.add_system_accounts_v2(config)?;

// Pack tree info and get output tree index
let output_state_tree_index = rpc
    .get_random_state_tree_info()?
    .pack_output_tree_index(&mut remaining_accounts)?;

// Pack validity proof tree infos
let packed_address_tree_info = rpc_result
    .pack_tree_infos(&mut remaining_accounts)
    .address_trees[0];

// Convert to account metas
let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

// Create instruction with both regular and remaining accounts
let instruction = Instruction {
    program_id: counter::ID,
    accounts: [
        accounts.to_account_metas(Some(true)),
        remaining_accounts_metas,
    ]
    .concat(),
    data: instruction_data.data(),
};

// Process the transaction
let signature = rpc.process_transaction(&[instruction]).await?;
```

### Important: Always Use `.mode_v2()`

**Every instruction builder must include `.mode_v2()` to use v2 accounts:**

```rust
// ✅ Correct - always include .mode_v2()
LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
    .mode_v2()  // Required for v2 accounts
    .with_light_account(counter)?
    .invoke(cpi_accounts)?;

// ❌ Wrong - missing .mode_v2()
LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
    .with_light_account(counter)?  // Will use v1 accounts
    .invoke(cpi_accounts)?;
```

## Complete Example Migration

### Before (Old SDK)
```rust
pub fn create_account<'info>(
    ctx: Context<'_, '_, '_, 'info, CreateAccount<'info>>,
    proof: ValidityProof,
    address_tree_info: PackedAddressTreeInfo,
) -> Result<()> {
    let cpi_accounts = CpiAccounts::new(
        ctx.accounts.signer.as_ref(),
        ctx.remaining_accounts,
        CPI_SIGNER,
    );

    let new_address_params = address_tree_info
        .into_new_address_params_packed(seed);

    let account = LightAccount::new_init(
        &crate::ID,
        Some(address),
        output_tree_index,
    );

    let cpi = CpiInputs::new_with_address(
        proof,
        vec![account.to_account_info().map_err(ProgramError::from)?],
        vec![new_address_params],
    );

    cpi.invoke_light_system_program(cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}
```

### After (SDK v2)
```rust
pub fn create_account<'info>(
    ctx: Context<'_, '_, '_, 'info, CreateAccount<'info>>,
    proof: ValidityProof,
    address_tree_info: PackedAddressTreeInfo,
) -> Result<()> {
    let cpi_accounts = CpiAccountsV2::new(
        ctx.accounts.signer.as_ref(),
        ctx.remaining_accounts,
        CPI_SIGNER,
    );

    let new_address_params = address_tree_info
        .into_new_address_params_assigned_packed(seed.into(), Some(0));

    let account = LightAccount::new_init(
        &crate::ID,
        Some(address),
        output_tree_index,
    );

    LightSystemProgramCpi::new_cpi(CPI_SIGNER, proof)
        .mode_v2()
        .with_light_account(account)?
        .with_new_addresses(&[new_address_params])
        .invoke(cpi_accounts)?;

    Ok(())
}
```

## Cargo.toml Dependencies

Update your dependencies to use the v2 SDK with the specific git commit:

```toml
[dependencies]
anchor-lang = "0.31.1"
light-hasher = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["solana"] }
light-sdk = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["anchor", "v2"] }
light-compressed-account = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["anchor"] }
light-batched-merkle-tree = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["solana"] }
light-sdk-types = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["anchor"] }

[dev-dependencies]
light-client = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f" }
light-program-test = { git = "https://github.com/Lightprotocol/light-protocol", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["v2"] }
tokio = "1.43.0"
solana-sdk = "2.2"
```

**Important**: Use the specific commit hash (`rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f"`) to ensure compatibility. The `main` branch may have breaking changes.

## Test Configuration for v2

When setting up tests, use the v2 configuration:

```rust
// Configure the test to use v2
let config = ProgramTestConfig::new_v2(true, Some(vec![("counter", counter::ID)]));
let mut rpc = LightProgramTest::new(config).await.unwrap();

// Get v2 address tree
let address_tree_info = rpc.get_address_tree_v2();
```

## Troubleshooting Common Migration Issues

### 1. **Dependencies Won't Compile**
```toml
# ❌ Wrong - using branch can lead to incompatible versions
light-sdk = { git = "...", branch = "main", features = ["anchor", "v2"] }

# ✅ Correct - use specific commit for stability
light-sdk = { git = "...", rev = "70a725b6dd44ce5b5c6e3b2ffc9ee53928027b8f", features = ["anchor", "v2"] }
```
**Solution**: After updating Cargo.toml, run `cargo update` to resolve conflicts.

### 2. **"Method not found" for `new_cpi` or `with_light_account`**
```rust
// ❌ Wrong - missing trait imports
LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
    .with_light_account(account)?  // Error: method not found
```
**Solution**: Add required trait imports INSIDE your program module:
```rust
#[program]
pub mod your_program {
    use light_sdk::cpi::{LightSystemProgramCpi, LightCpiInstruction, InvokeLightSystemProgram};
}
```

### 3. **Test Compilation Errors**
```rust
// ❌ Wrong - old test patterns
let compressed_account = rpc.get_compressed_account(address, None)
    .await.unwrap()
    .value;  // Error: Option<CompressedAccount> not CompressedAccount

// ✅ Correct - handle Option type
let compressed_account = rpc.get_compressed_account(address, None)
    .await.unwrap()
    .value
    .unwrap();  // Additional unwrap for Option
```

### 4. **Validity Proof Handling in Tests**
```rust
// ❌ Wrong - double unwrap
let rpc_result = rpc.get_validity_proof(...)
    .await?
    .value
    .unwrap();  // Error: ValidityProofWithContext doesn't have unwrap

// ✅ Correct - .value is sufficient
let rpc_result = rpc.get_validity_proof(...)
    .await?
    .value;  // No unwrap needed
```

### 5. **Account Meta Conversion**
```rust
// ❌ Wrong - old tuple access pattern
let instruction = Instruction {
    accounts: [
        accounts.to_account_metas(None),
        remaining_accounts.to_account_metas().0,  // Error: accessing tuple
    ].concat(),
};

// ✅ Correct - use destructuring
let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();
let instruction = Instruction {
    accounts: [
        accounts.to_account_metas(None),
        remaining_accounts_metas,
    ].concat(),
};
```

## Common Pitfalls

1. **Always include `.mode_v2()`** in instruction builders - this is REQUIRED for v2 accounts
2. **Use specific commit hash** in Cargo.toml dependencies, not `branch = "main"`
3. **Import traits inside program module** - `LightSystemProgramCpi`, `LightCpiInstruction`, `InvokeLightSystemProgram`
4. **Run `cargo update`** after changing dependencies to resolve version conflicts
5. **Use `add_system_accounts_v2()`** in tests, not the old `add_system_accounts()`
6. **Use `LightSystemProgramCpi::new_cpi()`** constructor, not `.new()`
7. **Update all test configurations** to v2 methods (`new_v2()`, `get_address_tree_v2()`)
8. **Handle Option types** in test results - `get_compressed_account` returns `Option<CompressedAccount>`
9. **Update address imports** from `address::v1::derive_address` to `address::v2::derive_address`

## Testing Your Migration

After migrating, ensure you:
1. Run all existing tests
2. Verify transaction building still works correctly
3. Check that CPI calls execute as expected
4. Validate error handling paths

## Need Help?

If you encounter issues during migration:
1. Check the working example in `counter/anchor-cpi-context/`
2. Refer to the Light Protocol documentation
3. Review the SDK source code for detailed API documentation