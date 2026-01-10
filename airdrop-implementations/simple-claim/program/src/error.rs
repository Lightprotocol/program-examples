use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Error, Debug, Copy, Clone)]
pub enum ClaimError {
    #[error("Missing required signature.")]
    MissingRequiredSignature,
    #[error("Tokens are still locked.")]
    TokensLocked,
    #[error("Invalid airdrop PDA provided.")]
    InvalidPDA,
}

impl From<ClaimError> for ProgramError {
    fn from(e: ClaimError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
