#![allow(unexpected_cfgs)]

use borsh::{BorshDeserialize, BorshSerialize};
use light_macros::pubkey_array;
use light_sdk_pinocchio::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    error::LightSdkError,
    instruction::{
        account_meta::{CompressedAccountMeta, CompressedAccountMetaClose},
        PackedAddressTreeInfo,
    },
    LightDiscriminator, LightHasher, ValidityProof,
};
use pinocchio::{
    account_info::AccountInfo, entrypoint, program_error::ProgramError, pubkey::Pubkey,
};

// Program ID for the counter program
pub const ID: Pubkey = pubkey_array!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");
// CPI signer derived from program ID for Light protocol interactions
pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

entrypoint!(process_instruction);

/// Instruction discriminators for the counter program
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

/// Compressed account state for the counter
/// Uses Light protocol for state compression
#[derive(
    Debug, Default, Clone, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct CounterAccount {
    /// Owner of the counter - used in address derivation and authorization
    #[hash]
    pub owner: Pubkey,
    /// Current counter value
    pub value: u64,
}

/// Instruction data for creating a new counter
#[derive(BorshSerialize, BorshDeserialize)]
pub struct CreateCounterInstructionData {
    /// Zero-knowledge proof for the transaction
    pub proof: ValidityProof,
    /// Information about the address tree for storing the new address
    pub address_tree_info: PackedAddressTreeInfo,
    /// Index of the state tree where the counter will be stored
    pub output_state_tree_index: u8,
}

/// Instruction data for modifying an existing counter
#[derive(BorshSerialize, BorshDeserialize)]
pub struct IncrementCounterInstructionData {
    /// Zero-knowledge proof for the transaction
    pub proof: ValidityProof,
    /// Current counter value (for verification)
    pub counter_value: u64,
    /// Metadata of the compressed account being modified
    pub account_meta: CompressedAccountMeta,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct DecrementCounterInstructionData {
    pub proof: ValidityProof,
    pub counter_value: u64,
    pub account_meta: CompressedAccountMeta,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct ResetCounterInstructionData {
    pub proof: ValidityProof,
    pub counter_value: u64,
    pub account_meta: CompressedAccountMeta,
}

/// Instruction data for closing a counter account
#[derive(BorshSerialize, BorshDeserialize)]
pub struct CloseCounterInstructionData {
    pub proof: ValidityProof,
    pub counter_value: u64,
    /// Close-specific metadata that handles account deletion
    pub account_meta: CompressedAccountMetaClose,
}

/// Program-specific error types
#[derive(Debug, Clone)]
pub enum CounterError {
    Unauthorized,
    Overflow,
    Underflow,
}

impl From<CounterError> for ProgramError {
    fn from(e: CounterError) -> Self {
        match e {
            CounterError::Unauthorized => ProgramError::Custom(1),
            CounterError::Overflow => ProgramError::Custom(2),
            CounterError::Underflow => ProgramError::Custom(3),
        }
    }
}

/// Main instruction handler - routes to specific instruction handlers
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    if program_id != &crate::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    // First byte is the instruction discriminator
    let discriminator = InstructionType::try_from(instruction_data[0])
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    match discriminator {
        InstructionType::CreateCounter => {
            let instruction_data =
                CreateCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            create_counter(accounts, instruction_data)
        }
        InstructionType::IncrementCounter => {
            let instruction_data =
                IncrementCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            increment_counter(accounts, instruction_data)
        }
        InstructionType::DecrementCounter => {
            let instruction_data =
                DecrementCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            decrement_counter(accounts, instruction_data)
        }
        InstructionType::ResetCounter => {
            let instruction_data =
                ResetCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            reset_counter(accounts, instruction_data)
        }
        InstructionType::CloseCounter => {
            let instruction_data =
                CloseCounterInstructionData::try_from_slice(&instruction_data[1..])
                    .map_err(|_| ProgramError::InvalidInstructionData)?;
            close_counter(accounts, instruction_data)
        }
    }
}

/// Creates a new counter account with compressed state
pub fn create_counter(
    accounts: &[AccountInfo],
    instruction_data: CreateCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    // Derive deterministic address based on signer and "counter" seed
    let (address, address_seed) = derive_address(
        &[b"counter", signer.key().as_ref()],
        &instruction_data
            .address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ProgramError::NotEnoughAccountKeys)?,
        &ID,
    );

    // Convert address tree info into parameters for creating new address
    let new_address_params = instruction_data
        .address_tree_info
        .into_new_address_params_packed(address_seed);

    // Initialize new compressed account
    let mut counter = LightAccount::<'_, CounterAccount>::new_init(
        &ID,
        Some(address),
        instruction_data.output_state_tree_index,
    );

    counter.owner = *signer.key();
    counter.value = 0;

    // Create CPI call to Light system program with new address
    let cpi = CpiInputs::new_with_address(
        instruction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
        vec![new_address_params],
    );
    cpi.invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Increments the counter value by 1 with overflow protection
pub fn increment_counter(
    accounts: &[AccountInfo],
    instruction_data: IncrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Load existing compressed account for mutation
    let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
        &ID,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    counter.value = counter.value.checked_add(1).ok_or(CounterError::Overflow)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let cpi_inputs = CpiInputs::new(
        instruction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );
    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Decrements the counter value by 1 with underflow protection
pub fn decrement_counter(
    accounts: &[AccountInfo],
    instruction_data: DecrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
        &ID,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    counter.value = counter
        .value
        .checked_sub(1)
        .ok_or(CounterError::Underflow)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let cpi_inputs = CpiInputs::new(
        instruction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );

    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Resets the counter value back to 0
pub fn reset_counter(
    accounts: &[AccountInfo],
    instruction_data: ResetCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
        &ID,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    counter.value = 0;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);
    let cpi_inputs = CpiInputs::new(
        instruction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );

    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}

/// Closes/deletes the counter account from compressed state
pub fn close_counter(
    accounts: &[AccountInfo],
    instruction_data: CloseCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    // Create account handle for closure (no mutation needed)
    let counter = LightAccount::<'_, CounterAccount>::new_close(
        &ID,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(ProgramError::from)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let cpi_inputs = CpiInputs::new(
        instruction_data.proof,
        vec![counter.to_account_info().map_err(ProgramError::from)?],
    );

    cpi_inputs
        .invoke_light_system_program(light_cpi_accounts)
        .map_err(ProgramError::from)?;

    Ok(())
}
