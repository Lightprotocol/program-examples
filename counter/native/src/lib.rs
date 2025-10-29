#![allow(unexpected_cfgs)]

use borsh::{BorshDeserialize, BorshSerialize};
use light_macros::pubkey;
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{
        v1::{CpiAccounts, LightSystemProgramCpi},
        CpiSigner, InvokeLightSystemProgram, LightCpiInstruction,
    },
    derive_light_cpi_signer,
    error::LightSdkError,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher,
};
use solana_program::{
    account_info::AccountInfo, entrypoint, program_error::ProgramError, pubkey::Pubkey,
};
pub const ID: Pubkey = pubkey!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");
pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

entrypoint!(process_instruction);

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
    if program_id != &crate::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let discriminator = InstructionType::try_from(instruction_data[0])
        .map_err(|_| ProgramError::InvalidInstructionData)?;

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

pub fn create_counter(
    accounts: &[AccountInfo],
    instuction_data: CreateCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    let (address, address_seed) = derive_address(
        &[b"counter", signer.key.as_ref()],
        &instuction_data
            .address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ProgramError::NotEnoughAccountKeys)?,
        &ID,
    );

    let new_address_params = instuction_data
        .address_tree_info
        .into_new_address_params_packed(address_seed);

    let mut counter = LightAccount::<CounterAccount>::new_init(
        &ID,
        Some(address),
        instuction_data.output_state_tree_index,
    );
    counter.owner = *signer.key;
    counter.value = 0;

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instuction_data.proof)
        .with_light_account(counter)?
        .with_new_addresses(&[new_address_params])
        .invoke(light_cpi_accounts)?;

    Ok(())
}

pub fn increment_counter(
    accounts: &[AccountInfo],
    instuction_data: IncrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let mut counter = LightAccount::<CounterAccount>::new_mut(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )?;

    counter.value = counter.value.checked_add(1).ok_or(CounterError::Overflow)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instuction_data.proof)
        .with_light_account(counter)?
        .invoke(light_cpi_accounts)?;

    Ok(())
}

pub fn decrement_counter(
    accounts: &[AccountInfo],
    instuction_data: DecrementCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let mut counter = LightAccount::<CounterAccount>::new_mut(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )?;

    counter.value = counter
        .value
        .checked_sub(1)
        .ok_or(CounterError::Underflow)?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instuction_data.proof)
        .with_light_account(counter)?
        .invoke(light_cpi_accounts)?;

    Ok(())
}

pub fn reset_counter(
    accounts: &[AccountInfo],
    instuction_data: ResetCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let mut counter = LightAccount::<CounterAccount>::new_mut(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )?;

    counter.value = 0;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);
    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instuction_data.proof)
        .with_light_account(counter)?
        .invoke(light_cpi_accounts)?;

    Ok(())
}

pub fn close_counter(
    accounts: &[AccountInfo],
    instuction_data: CloseCounterInstructionData,
) -> Result<(), ProgramError> {
    let signer = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;

    let counter = LightAccount::<CounterAccount>::new_close(
        &ID,
        &instuction_data.account_meta,
        CounterAccount {
            owner: *signer.key,
            value: instuction_data.counter_value,
        },
    )?;

    let light_cpi_accounts = CpiAccounts::new(signer, &accounts[1..], LIGHT_CPI_SIGNER);

    LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, instuction_data.proof)
        .with_light_account(counter)?
        .invoke(light_cpi_accounts)?;

    Ok(())
}
