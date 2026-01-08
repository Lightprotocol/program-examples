#![allow(unexpected_cfgs)]

use borsh::{BorshDeserialize, BorshSerialize};
use light_macros::pubkey_array;
use light_sdk_pinocchio::{
    address::v1::derive_address,
    constants::ADDRESS_TREE_V1,
    cpi::{
        v1::{CpiAccounts, LightSystemProgramCpi},
        CpiAccountsConfig, CpiSigner, InvokeLightSystemProgram, LightCpiInstruction,
    },
    derive_light_cpi_signer,
    error::LightSdkError,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightAccount, LightDiscriminator, LightHasher,
};
use pinocchio::{
    account_info::AccountInfo, entrypoint, program_error::ProgramError, pubkey::Pubkey,
};

pub const ID: Pubkey = pubkey_array!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");
pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

entrypoint!(process_instruction);

fn to_custom_error<E: Into<u64>>(e: E) -> ProgramError {
    ProgramError::Custom(e.into() as u32)
}

fn to_custom_error_u32<E: Into<u32>>(e: E) -> ProgramError {
    ProgramError::Custom(e.into())
}

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

#[derive(
    Debug, Default, Clone, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct CounterAccount {
    #[hash]
    pub owner: Pubkey,
    pub value: u64,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct CreateCounterInstructionData {
    pub proof: ValidityProof,
    pub address_tree_info: PackedAddressTreeInfo,
    pub output_state_tree_index: u8,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct IncrementCounterInstructionData {
    pub proof: ValidityProof,
    pub counter_value: u64,
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

#[derive(BorshSerialize, BorshDeserialize)]
pub struct CloseCounterInstructionData {
    pub proof: ValidityProof,
    pub counter_value: u64,
    pub account_meta: CompressedAccountMeta,
}

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

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    if program_id != &Pubkey::from(crate::ID) {
        return Err(ProgramError::IncorrectProgramId);
    }
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let discriminator = InstructionType::try_from(instruction_data[0])
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let result = match discriminator {
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
    };

    result.map_err(|e| ProgramError::Custom(u64::from(e) as u32))
}

pub fn create_counter(
    accounts: &[AccountInfo],
    instruction_data: CreateCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let config = CpiAccountsConfig::new(LIGHT_CPI_SIGNER);
    let cpi_accounts = CpiAccounts::try_new_with_config(signer, &accounts[1..], config)
        .map_err(to_custom_error_u32)?;

    let tree_pubkey = cpi_accounts
        .get_tree_account_info(
            instruction_data
                .address_tree_info
                .address_merkle_tree_pubkey_index as usize,
        )
        .map_err(to_custom_error_u32)?
        .key();

    if *tree_pubkey != ADDRESS_TREE_V1 {
        pinocchio::log::sol_log("Invalid address tree");
        return Err(ProgramError::InvalidAccountData);
    }

    let program_id = Pubkey::from(ID);
    let (address, address_seed) = derive_address(
        &[b"counter", signer.key().as_ref()],
        tree_pubkey,
        &program_id,
    );

    let new_address_params = instruction_data
        .address_tree_info
        .into_new_address_params_packed(address_seed);

    let mut counter = LightAccount::<CounterAccount>::new_init(
        &program_id,
        Some(address),
        instruction_data.output_state_tree_index,
    );

    counter.owner = *signer.key();
    counter.value = 0;

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instruction_data.proof)
        .with_light_account(counter)
        .map_err(to_custom_error)?
        .with_new_addresses(&[new_address_params])
        .invoke(cpi_accounts)
}

pub fn increment_counter(
    accounts: &[AccountInfo],
    instruction_data: IncrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let program_id = Pubkey::from(ID);
    let mut counter = LightAccount::<CounterAccount>::new_mut(
        &program_id,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(|e| ProgramError::Custom(u64::from(e) as u32))?;

    counter.value = counter.value.checked_add(1).ok_or(CounterError::Overflow)?;

    let config = CpiAccountsConfig::new(LIGHT_CPI_SIGNER);
    let cpi_accounts = CpiAccounts::try_new_with_config(signer, &accounts[1..], config)
        .map_err(to_custom_error_u32)?;

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instruction_data.proof)
        .with_light_account(counter)
        .map_err(to_custom_error)?
        .invoke(cpi_accounts)
}

pub fn decrement_counter(
    accounts: &[AccountInfo],
    instruction_data: DecrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let program_id = Pubkey::from(ID);
    let mut counter = LightAccount::<CounterAccount>::new_mut(
        &program_id,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(|e| ProgramError::Custom(u64::from(e) as u32))?;

    counter.value = counter
        .value
        .checked_sub(1)
        .ok_or(CounterError::Underflow)?;

    let config = CpiAccountsConfig::new(LIGHT_CPI_SIGNER);
    let cpi_accounts = CpiAccounts::try_new_with_config(signer, &accounts[1..], config)
        .map_err(to_custom_error_u32)?;

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instruction_data.proof)
        .with_light_account(counter)
        .map_err(to_custom_error)?
        .invoke(cpi_accounts)
}

pub fn reset_counter(
    accounts: &[AccountInfo],
    instruction_data: ResetCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(LightSdkError::ProgramError(
        ProgramError::NotEnoughAccountKeys,
    ))?;

    let program_id = Pubkey::from(ID);
    let mut counter = LightAccount::<CounterAccount>::new_mut(
        &program_id,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(|e| ProgramError::Custom(u64::from(e) as u32))?;

    counter.value = 0;

    let config = CpiAccountsConfig::new(LIGHT_CPI_SIGNER);
    let cpi_accounts = CpiAccounts::try_new_with_config(signer, &accounts[1..], config)
        .map_err(to_custom_error_u32)?;

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instruction_data.proof)
        .with_light_account(counter)
        .map_err(to_custom_error)?
        .invoke(cpi_accounts)
}

pub fn close_counter(
    accounts: &[AccountInfo],
    instruction_data: CloseCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let program_id = Pubkey::from(ID);
    let counter = LightAccount::<CounterAccount>::new_close(
        &program_id,
        &instruction_data.account_meta,
        CounterAccount {
            owner: *signer.key(),
            value: instruction_data.counter_value,
        },
    )
    .map_err(|e| ProgramError::Custom(u64::from(e) as u32))?;

    let config = CpiAccountsConfig::new(LIGHT_CPI_SIGNER);
    let cpi_accounts = CpiAccounts::try_new_with_config(signer, &accounts[1..], config)
        .map_err(to_custom_error_u32)?;

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instruction_data.proof)
        .with_light_account(counter)
        .map_err(to_custom_error)?
        .invoke(cpi_accounts)
}
