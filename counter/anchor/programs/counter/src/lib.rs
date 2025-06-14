#![allow(unexpected_cfgs)]

use anchor_lang::{prelude::*, AnchorDeserialize, Discriminator};
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    instruction::{
        account_meta::{CompressedAccountMeta, CompressedAccountMetaClose},
        PackedAddressTreeInfo, ValidityProof,
    },
    LightDiscriminator, LightHasher,
};

/// Program ID for the Light Protocol counter program
declare_id!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

/// CPI signer derived from the program ID, used for Light Protocol cross-program invocations
pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

/// Counter program implementing Light Protocol compressed state management
///
/// This program demonstrates how to work with Light Protocol's compressed accounts,
/// which provide state compression benefits similar to Solana's account compression
/// but for arbitrary program state rather than just NFT metadata.
#[program]
pub mod counter {

    use super::*;

    /// Creates a new counter account using Light Protocol's compressed state
    ///
    /// This instruction initializes a new counter with value 0. Unlike traditional Solana accounts,
    /// the counter state is compressed using Merkle trees, significantly reducing on-chain storage costs.
    ///
    /// # Arguments
    /// * `ctx` - Standard Anchor context with signer account
    /// * `proof` - Zero-knowledge proof validating the state transition
    /// * `address_tree_info` - Information about the address tree for compressed account addressing
    /// * `output_state_tree_index` - Index in the state tree where the new account will be stored
    pub fn create_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
    ) -> Result<()> {
        // Set up CPI accounts for interacting with the Light system program
        // This is analogous to setting up accounts for a regular Solana CPI call
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Derive a deterministic address for the counter based on the signer's pubkey
        // This creates a PDA-like address that's unique per user
        let (address, address_seed) = derive_address(
            &[b"counter", ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        // Pack the address parameters for the new account creation
        let new_address_params = address_tree_info.into_new_address_params_packed(address_seed);

        // Initialize a new Light account - this is similar to Account::try_from_slice() in regular Anchor
        // but for compressed accounts that exist in Merkle trees rather than as individual accounts
        let mut counter = LightAccount::<'_, CounterAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        // Set initial values for the counter
        counter.owner = ctx.accounts.signer.key();
        counter.value = 0;

        // Create the CPI inputs with the proof and account data
        // This is equivalent to preparing instruction data for a regular Solana CPI
        let cpi = CpiInputs::new_with_address(
            proof,
            vec![counter.to_account_info().map_err(ProgramError::from)?],
            vec![new_address_params],
        );

        // Invoke the Light system program to create the compressed account
        // This is like calling invoke() but for Light Protocol's compressed state system
        cpi.invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }

    /// Increments the counter value by 1
    ///
    /// This demonstrates updating compressed state. The instruction reads the current compressed
    /// account state, modifies it, and writes it back to the Merkle tree with a new proof.
    ///
    /// # Arguments
    /// * `ctx` - Standard Anchor context
    /// * `proof` - Zero-knowledge proof for the state transition
    /// * `counter_value` - Current value of the counter (for verification)
    /// * `account_meta` - Metadata about the compressed account being modified
    pub fn increment_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        // Create a mutable reference to the compressed account
        // This loads the current state from the Merkle tree and prepares it for modification
        // Similar to how Account<'info, T>::try_from() works in regular Anchor programs
        let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

        // Log current state for debugging (similar to msg! in regular Solana programs)
        msg!("counter {}", counter.value);
        msg!("counter {:?}", counter);

        // Perform the increment with overflow protection
        counter.value = counter.value.checked_add(1).ok_or(CustomError::Overflow)?;

        // Set up CPI accounts for the Light system program call
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Prepare the CPI inputs with the modified account state
        let cpi_inputs = CpiInputs::new(
            proof,
            vec![counter.to_account_info().map_err(ProgramError::from)?],
        );

        // Commit the state change to the compressed account system
        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;
        Ok(())
    }

    /// Decrements the counter value by 1
    ///
    /// Similar to increment but subtracts 1 from the counter value with underflow protection.
    ///
    /// # Arguments
    /// * `ctx` - Standard Anchor context
    /// * `proof` - Zero-knowledge proof for the state transition
    /// * `counter_value` - Current value of the counter (for verification)
    /// * `account_meta` - Metadata about the compressed account being modified
    pub fn decrement_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        // Load the compressed account for modification
        let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

        // Perform the decrement with underflow protection
        counter.value = counter.value.checked_sub(1).ok_or(CustomError::Underflow)?;

        // Set up and execute the CPI to commit the change
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let cpi_inputs = CpiInputs::new(
            proof,
            vec![counter.to_account_info().map_err(ProgramError::from)?],
        );

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }

    /// Resets the counter value to 0
    ///
    /// This instruction sets the counter back to its initial value of 0.
    ///
    /// # Arguments
    /// * `ctx` - Standard Anchor context
    /// * `proof` - Zero-knowledge proof for the state transition
    /// * `counter_value` - Current value of the counter (for verification)
    /// * `account_meta` - Metadata about the compressed account being modified
    pub fn reset_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        // Load the compressed account for modification
        let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

        // Reset the counter to 0
        counter.value = 0;

        // Set up and execute the CPI to commit the change
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );
        let cpi_inputs = CpiInputs::new(
            proof,
            vec![counter.to_account_info().map_err(ProgramError::from)?],
        );

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }

    /// Closes the counter account and reclaims storage
    ///
    /// This instruction permanently deletes the compressed counter account.
    /// Unlike regular Solana accounts, closed compressed accounts cannot be recreated
    /// at the same address due to the Merkle tree structure.
    ///
    /// # Arguments
    /// * `ctx` - Standard Anchor context
    /// * `proof` - Zero-knowledge proof for the account closure
    /// * `counter_value` - Current value of the counter (for verification)
    /// * `account_meta` - Metadata about the compressed account being closed
    pub fn close_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMetaClose,
    ) -> Result<()> {
        // Create a close operation for the compressed account
        // This is similar to calling close() on a regular Anchor account
        // but works with the compressed account system
        let counter = LightAccount::<'_, CounterAccount>::new_close(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

        // Set up and execute the CPI to close the account
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let cpi_inputs = CpiInputs::new(
            proof,
            vec![counter.to_account_info().map_err(ProgramError::from)?],
        );

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;
        Ok(())
    }
}

/// Custom error codes for the counter program
///
/// These follow the same pattern as regular Anchor error codes
/// but are specific to counter operations.
#[error_code]
pub enum CustomError {
    #[msg("No authority to perform this action")]
    Unauthorized,
    #[msg("Counter overflow")]
    Overflow,
    #[msg("Counter underflow")]
    Underflow,
}

/// Generic account structure for instructions
///
/// This is a standard Anchor accounts struct that requires a mutable signer.
/// The signer pays for transaction fees and must have write permissions.
#[derive(Accounts)]
pub struct GenericAnchorAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

/// Counter account data structure for compressed state
///
/// This struct defines the data that gets compressed and stored in the Merkle tree.
/// The #[hash] attribute on owner means it's included in the account's hash computation,
/// while value is not hashed (allowing for more efficient updates).
///
/// The #[event] attribute makes this struct part of the IDL for client integration.
#[event]
#[derive(Clone, Debug, Default, LightDiscriminator, LightHasher)]
pub struct CounterAccount {
    /// The public key of the account owner (hashed for security)
    #[hash]
    pub owner: Pubkey,
    /// Current counter value (not hashed for efficiency)
    pub value: u64,
}
