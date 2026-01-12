use anchor_lang::prelude::*;

#[error_code]
pub enum AirdropError {
    #[msg("Tokens are still locked")]
    TokensLocked,
    #[msg("Invalid ZK proof")]
    InvalidProof,
    #[msg("Airdrop is not active")]
    AirdropNotActive,
    #[msg("Invalid eligibility root")]
    InvalidEligibilityRoot,
    #[msg("Invalid nullifier")]
    InvalidNullifier,
    #[msg("Invalid address tree")]
    InvalidAddressTree,
    #[msg("Not enough accounts provided")]
    AccountNotEnoughKeys,
    #[msg("Recipient mismatch")]
    RecipientMismatch,
    #[msg("Amount mismatch")]
    AmountMismatch,
}
