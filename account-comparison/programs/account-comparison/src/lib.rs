// Import Anchor framework for Solana program development
use anchor_lang::prelude::*;
// Import Light SDK components for ZK Compression functionality
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher,
};

// Define custom error codes for the program
#[error_code]
pub enum CustomError {
    #[msg("No authority to perform this action")]
    Unauthorized,
}

// Declare the program ID - this uniquely identifies the program on Solana
declare_id!("FYX4GmKJYzSiycc7XZKf12NGXNE9siSx1cJubYJniHcv");

// Derive a CPI signer for Light system program interactions
// This is used to sign transactions when calling the Light system program
const CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("FYX4GmKJYzSiycc7XZKf12NGXNE9siSx1cJubYJniHcv");

#[program]
pub mod account_comparison {
    use light_sdk::error::LightSdkError;

    use super::*;

    // Create a regular Solana account (stores data on-chain)
    // This function demonstrates traditional account creation with rent costs
    pub fn create_account(ctx: Context<CreateAccount>, name: String) -> Result<()> {
        // Get mutable reference to the account being created
        let account = &mut ctx.accounts.account;
        // Initialize account data with default values
        account.data = [1; 128];
        account.name = name;
        // Set the account owner to the user who created it
        account.user = *ctx.accounts.user.key;

        Ok(())
    }

    // Update data in a regular Solana account
    // This function modifies existing account data on-chain
    pub fn update_data(ctx: Context<UpdateData>, data: [u8; 128]) -> Result<()> {
        // Get mutable reference to the account and update its data
        let account = &mut ctx.accounts.account;
        account.data = data;
        Ok(())
    }

    // Create a compressed account using ZK Compression
    // This stores account data off-chain with cryptographic proofs for verification
    pub fn create_compressed_account<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateCompressedAccount<'info>>,
        name: String,
        proof: ValidityProof,                    // Zero-knowledge proof for state transitions
        address_tree_info: PackedAddressTreeInfo, // Information about the address tree
        output_tree_index: u8,                  // Index in the output state tree
    ) -> Result<()> {
        // Set up CPI accounts for calling the Light system program
        // This includes the user account and remaining accounts for tree operations
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.user.as_ref(),
            ctx.remaining_accounts,
            CPI_SIGNER,
        );

        // Derive a deterministic address for the compressed account
        // Uses a seed based on the user's public key to ensure uniqueness
        let (address, address_seed) = derive_address(
            &[b"account", ctx.accounts.user.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|err| ProgramError::from(LightSdkError::from(err)))?,
            &crate::ID,
        );

        // Create a new compressed account with empty input state
        // LightAccount::new_init initializes an account for creation
        // The account data will be hashed using Poseidon hashes and stored off-chain
        let mut compressed_account = LightAccount::<'_, CompressedAccountData>::new_init(
            &crate::ID,           // Program ID that owns this account
            Some(address),        // Derived address for the account
            output_tree_index,    // Position in the state tree
        );

        // Set the compressed account data
        compressed_account.user = ctx.accounts.user.key();
        compressed_account.name = name;
        compressed_account.data = [1u8; 128];

        // Prepare address parameters for the new account creation
        let new_address_params = address_tree_info.into_new_address_params_packed(address_seed);

        // Create CPI inputs with the validity proof and account information
        // This includes the compressed account converted to account info format
        let cpi = CpiInputs::new_with_address(
            proof,
            vec![compressed_account
                .to_account_info()
                .map_err(ProgramError::from)?],
            vec![new_address_params],
        );
        
        // Invoke the Light system program to create the compressed account
        // This updates the state trees and stores the commitment on-chain
        cpi.invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }

    // Update an existing compressed account
    // This function demonstrates how to modify compressed account data
    pub fn update_compressed_account<'info>(
        ctx: Context<'_, '_, '_, 'info, UpdateCompressedAccount<'info>>,
        new_data: [u8; 128],                    // New data to store in the account
        existing_data: [u8; 128],               // Current data (for verification)
        name: String,                           // Account name
        proof: ValidityProof,                   // Proof that the update is valid
        account_meta: CompressedAccountMeta,    // Metadata about the compressed account
    ) -> Result<()> {
        // Create a mutable reference to the existing compressed account
        // new_mut loads an existing account for modification
        let mut compressed_account = LightAccount::<'_, CompressedAccountData>::new_mut(
            &crate::ID,
            &account_meta,
            // Provide the current account data for verification
            CompressedAccountData {
                user: ctx.accounts.user.key(),
                data: existing_data,
                name,
            },
        )
        .map_err(ProgramError::from)?;

        // Verify that the user has authority to update this account
        if compressed_account.user != ctx.accounts.user.key() {
            return err!(CustomError::Unauthorized);
        }

        // Update the account data
        compressed_account.data = new_data;

        // Set up CPI accounts for the Light system program call
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.user.as_ref(),
            ctx.remaining_accounts,
            CPI_SIGNER,
        );

        // Create CPI inputs with the validity proof and updated account
        let cpi_inputs = CpiInputs::new(
            proof,
            vec![compressed_account
                .to_account_info()
                .map_err(ProgramError::from)?],
        );

        // Invoke the Light system program to update the compressed account
        // This creates a new state commitment and nullifies the old one
        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }
}

// Account validation struct for creating regular Solana accounts
#[derive(Accounts)]
pub struct CreateAccount<'info> {
    #[account(mut)]
    pub user: Signer<'info>,                   // User who pays for and owns the account
    // Initialize a new account with:
    // - payer: user pays the rent
    // - space: 8 bytes (discriminator) + 32 (pubkey) + 128 (data) + 64 (string)
    // - seeds: deterministic address based on user's pubkey
    #[account(init, payer = user, space = 8 + 32 + 128 + 64, seeds = [b"account", user.key().as_ref()], bump)]
    pub account: Account<'info, AccountData>,
    pub system_program: Program<'info, System>, // System program for account creation
}

// Data structure for regular Solana accounts
// Memory layout: [0..8] discriminator, [8..40] user pubkey, [40..168] name, [168..232] data
#[account]
#[derive(Debug)]
pub struct AccountData {
    pub user: Pubkey,       // Owner of the account (32 bytes)
    pub name: String,       // Account name (variable length, up to 64 bytes)
    pub data: [u8; 128],    // Account data (128 bytes)
}

// Account validation struct for updating regular accounts
#[derive(Accounts)]
pub struct UpdateData<'info> {
    #[account(mut, has_one = user)]            // Account must be owned by the user
    pub account: Account<'info, AccountData>,
    #[account(mut)]
    pub user: Signer<'info>,                   // User must sign the transaction
}

// Account validation struct for creating compressed accounts
// Note: Much simpler than regular accounts - no space allocation needed
#[derive(Accounts)]
pub struct CreateCompressedAccount<'info> {
    #[account(mut)]
    pub user: Signer<'info>,                   // Only need the user to sign
}

// Account validation struct for updating compressed accounts
#[derive(Accounts)]
pub struct UpdateCompressedAccount<'info> {
    #[account(mut)]
    pub user: Signer<'info>,                   // Only need the user to sign
}

// Data structure for compressed accounts
// This struct is serialized and hashed using Poseidon hashes
// The #[hash] attribute marks fields that are included in the account hash
#[derive(Clone, Debug, AnchorDeserialize, AnchorSerialize, LightDiscriminator, LightHasher)]
pub struct CompressedAccountData {
    #[hash]
    pub user: Pubkey,       // Owner of the account (included in hash)
    #[hash]
    pub name: String,       // Account name (included in hash)
    #[hash]
    pub data: [u8; 128],    // Account data (included in hash)
}

// Default implementation for compressed account data
impl Default for CompressedAccountData {
    fn default() -> Self {
        Self {
            user: Pubkey::default(),
            name: String::default(),
            data: [0u8; 128],
        }
    }
}
