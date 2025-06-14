//! # Light Counter Program
//!
//! A Solana program demonstrating compressed state management using the Light Protocol SDK.
//! This program implements a simple counter that can be created, incremented, decremented,
//! reset, and closed using compressed accounts for efficient state storage.

#![allow(unexpected_cfgs)]

use borsh::{BorshDeserialize, BorshSerialize};
use light_macros::pubkey;
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    error::LightSdkError,
    instruction::{
        account_meta::{CompressedAccountMeta, CompressedAccountMetaClose},
        PackedAddressTreeInfo, ValidityProof,
    },
    LightDiscriminator, LightHasher,
};
use solana_program::{
    account_info::AccountInfo, entrypoint, program_error::ProgramError, pubkey::Pubkey,
};

/// Program ID for the Light Counter program
pub const ID: Pubkey = pubkey!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

/// CPI signer derived from the program ID for Light Protocol operations
pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

entrypoint!(process_instruction);

/// Instruction discriminators for different counter operations
///
/// Similar to Anchor's instruction discriminators, these are used to route
/// incoming instructions to the appropriate handler function.
#[repr(u8)]
pub enum InstructionType {
    CreateCounter = 0,
    IncrementCounter = 1,
    DecrementCounter = 2,
    ResetCounter = 3,
    CloseCounter = 4,
}

impl TryFrom<u8> for InstructionType {
    type Error = LightSdkError;

    /// Converts a u8 discriminator to an InstructionType
    ///
    /// This is similar to how Anchor handles instruction routing, but manually implemented
    /// for Light Protocol compressed state programs.
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(InstructionType::CreateCounter),
            1 => Ok(InstructionType::IncrementCounter),
            2 => Ok(InstructionType::DecrementCounter),
            3 => Ok(InstructionType::ResetCounter),
            4 => Ok(InstructionType::CloseCounter),
            _ => panic!("Invalid instruction discriminator."),
        }
    }
}

/// The main account structure for our counter
///
/// This is similar to an Anchor account struct, but uses Light Protocol traits
/// for compressed state management. The `#[hash]` attribute on owner means
/// this field is included in the account's hash calculation.
#[derive(
    Debug, Default, Clone, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct CounterAccount {
    /// The owner of this counter (included in hash for security)
    #[hash]
    pub owner: Pubkey,
    /// The current counter value
    pub value: u64,
}

/// Instruction data for creating a new counter
///
/// Contains the validity proof and tree information needed for compressed account creation
#[derive(BorshSerialize, BorshDeserialize)]
pub struct CreateCounterInstructionData {
    /// Zero-knowledge proof validating the transaction
    pub proof: ValidityProof,
    /// Information about the address tree where the counter will be stored
    pub address_tree_info: PackedAddressTreeInfo,
    /// Index of the state tree for output accounts
    pub output_state_tree_index: u8,
}

/// Instruction data for incrementing a counter
#[derive(BorshSerialize, BorshDeserialize)]
pub struct IncrementCounterInstructionData {
    /// Zero-knowledge proof validating the transaction
    pub proof: ValidityProof,
    /// Current counter value (for verification)
    pub counter_value: u64,
    /// Metadata for the compressed account being modified
    pub account_meta: CompressedAccountMeta,
}

/// Instruction data for decrementing a counter
#[derive(BorshSerialize, BorshDeserialize)]
pub struct DecrementCounterInstructionData {
    /// Zero-knowledge proof validating the transaction
    pub proof: ValidityProof,
    /// Current counter value (for verification)
    pub counter_value: u64,
    /// Metadata for the compressed account being modified
    pub account_meta: CompressedAccountMeta,
}

/// Instruction data for resetting a counter to zero
#[derive(BorshSerialize, BorshDeserialize)]
pub struct ResetCounterInstructionData {
    /// Zero-knowledge proof validating the transaction
    pub proof: ValidityProof,
    /// Current counter value (for verification)
    pub counter_value: u64,
    /// Metadata for the compressed account being modified
    pub account_meta: CompressedAccountMeta,
}

/// Instruction data for closing a counter account
#[derive(BorshSerialize, BorshDeserialize)]
pub struct CloseCounterInstructionData {
    /// Zero-knowledge proof validating the transaction
    pub proof: ValidityProof,
    /// Current counter value (for verification)
    pub counter_value: u64,
    /// Metadata for the compressed account being closed
    pub account_meta: CompressedAccountMetaClose,
}

/// Custom error types for the counter program
///
/// Similar to Anchor's error handling, but mapped to ProgramError for compatibility
#[derive(Debug, Clone)]
pub enum CounterError {
    Unauthorized,
    Overflow,
    Underflow,
}

impl From<CounterError> for ProgramError {
    /// Converts our custom errors to ProgramError with specific error codes
    fn from(e: CounterError) -> Self {
        match e {
            CounterError::Unauthorized => ProgramError::Custom(1),
            CounterError::Overflow => ProgramError::Custom(2),
            CounterError::Underflow => ProgramError::Custom(3),
        }
    }
}

/// Main instruction processor - similar to Anchor's instruction handler
///
/// Routes incoming instructions based on the discriminator byte, similar to how
/// Anchor programs work but with manual instruction parsing for Light Protocol.
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    // Verify this instruction is for our program
    if program_id != &crate::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    // Parse the instruction discriminator (first byte)
    let discriminator = InstructionType::try_from(instruction_data[0])
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    // Route to appropriate handler based on discriminator
    match discriminator {
        InstructionType::CreateCounter => {
            let instuction_data =
                CreateCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            create_counter(accounts, instuction_data)
        }
        InstructionType::IncrementCounter => {
            let instuction_data =
                IncrementCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            increment_counter(accounts, instuction_data)
        }
        InstructionType::DecrementCounter => {
            let instuction_data =
                DecrementCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            decrement_counter(accounts, instuction_data)
        }
        InstructionType::ResetCounter => {
            let instuction_data =
                ResetCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            reset_counter(accounts, instuction_data)
        }
        InstructionType::CloseCounter => {
            let instuction_data =
                CloseCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            close_counter(accounts, instuction_data)
        }
    }
}

/// Creates a new counter account with compressed state
///
/// This is similar to an Anchor `init` constraint, but uses Light Protocol's
/// compressed account system. The counter is created with a PDA-like address
/// derived from the signer's pubkey and a "counter" seed.
pub fn create_counter(
    accounts: &[AccountInfo],
    instuction_data: CreateCounterInstructionData,
) -> Result<(), ProgramError> {
    // First account must be the signer (similar to Anchor's Signer account)
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Set up CPI accounts for Light Protocol system program calls
    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    // Derive a deterministic address for this counter (like a PDA)
    // Uses "counter" + signer's pubkey as seeds
    let (address, address_seed) = derive_address(
        &[b"counter", signer.key.as_ref()],
        &instuction_data
            .address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ProgramError::NotEnoughAccountKeys)?,
        &ID,
    );

    // Prepare address parameters for the new account
    let new_address_params = instuction_data
        .address_tree_info
        .into_new_address_params_packed(address_seed);

    // Initialize the counter account with compressed state
    let mut counter = LightAccount::<'_, CounterAccount>::new_init(
        &ID,
        Some(address),
        instuction_data.output_state_tree_index,
    );
    counter.owner = *signer.key;
    counter.value = 0;

    // Execute the CPI to Light system program to create the compressed account
    let cpi = CpiInputs::new_with_address(
        instuction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
        vec![new_address_params],
    );
    cpi.invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Increments the counter value by 1
///
/// This demonstrates updating compressed state - the account is loaded,
/// modified, and then committed back to the compressed state tree.
pub fn increment_counter(
    accounts: &[AccountInfo],
    instuction_data: IncrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Load the existing counter account for mutation
    // The current value is provided in instruction data for verification
    let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    // Safely increment the counter, preventing overflow
    counter.value = counter.value.checked_add(1).ok_or(CounterError::Overflow)?;

    // Set up CPI accounts and execute the state update
    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let cpi_inputs = CpiInputs::new(
        instuction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );
    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Decrements the counter value by 1
///
/// Similar to increment but with underflow protection
pub fn decrement_counter(
    accounts: &[AccountInfo],
    instuction_data: DecrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Load the existing counter account for mutation
    let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    // Safely decrement the counter, preventing underflow
    counter.value = counter
        .value
        .checked_sub(1)
        .ok_or(CounterError::Underflow)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let cpi_inputs = CpiInputs::new(
        instuction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );

    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Resets the counter value back to 0
///
/// Demonstrates arbitrary state updates in compressed accounts
pub fn reset_counter(
    accounts: &[AccountInfo],
    instuction_data: ResetCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Load the existing counter account for mutation
    let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    // Reset counter to zero
    counter.value = 0;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);
    let cpi_inputs = CpiInputs::new(
        instuction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );

    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Closes the counter account, removing it from compressed state
///
/// This is similar to Anchor's `close` constraint - the account is marked
/// for closure and removed from the state tree.
pub fn close_counter(
    accounts: &[AccountInfo],
    instuction_data: CloseCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Load the counter account for closure (note: new_close instead of new_mut)
    let counter = LightAccount::<'_, CounterAccount>::new_close(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let cpi_inputs = CpiInputs::new(
        instuction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );

    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}
