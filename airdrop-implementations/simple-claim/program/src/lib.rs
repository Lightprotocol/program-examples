//! Program entrypoint
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if program_id != &crate::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    crate::processor::process_instruction(accounts, instruction_data)
}

mod error;
pub mod instruction;
pub mod processor;
pub use solana_program;

solana_program::declare_id!("7UHB3CfWv7SugNhfdyP7aeZJPMjnpd9zJ7xYkHozB3Na");
