#![allow(unexpected_cfgs)]

use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use borsh::{BorshDeserialize, BorshSerialize};
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher,
};

declare_id!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

pub const FIRST_SEED: &[u8] = b"first";
pub const SECOND_SEED: &[u8] = b"second";

#[program]
pub mod create_and_update {

    use super::*;

    /// Creates a new compressed account with initial data
    pub fn create_compressed_account<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        message: String,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let (address, address_seed) = derive_address(
            &[FIRST_SEED, ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut data_account = LightAccount::<'_, DataAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        data_account.owner = ctx.accounts.signer.key();
        data_account.message = message;
        msg!(
            "Created compressed account with message: {}",
            data_account.message
        );
        let cpi_inputs = CpiInputs::new_with_address(
            proof,
            vec![data_account.to_account_info().map_err(ProgramError::from)?],
            vec![address_tree_info.into_new_address_params_packed(address_seed)],
        );

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }

    /// Creates a new compressed account and updates an existing one in a single instruction
    pub fn create_and_update<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        existing_account: ExistingCompressedAccountIxData,
        new_account: NewCompressedAccountIxData,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Create new compressed account
        let (new_address, new_address_seed) = derive_address(
            &[SECOND_SEED, ctx.accounts.signer.key().as_ref()],
            &new_account
                .address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut new_data_account = LightAccount::<'_, DataAccount>::new_init(
            &crate::ID,
            Some(new_address),
            existing_account.account_meta.output_state_tree_index,
        );
        new_data_account.owner = ctx.accounts.signer.key();
        new_data_account.message = new_account.message.clone();

        let mut updated_data_account = LightAccount::<'_, DataAccount>::new_mut(
            &crate::ID,
            &existing_account.account_meta,
            DataAccount {
                owner: ctx.accounts.signer.key(),
                message: existing_account.message.clone(),
            },
        )
        .map_err(ProgramError::from)?;

        // Update the message
        updated_data_account.message = existing_account.update_message.clone();

        let cpi_inputs = CpiInputs::new_with_address(
            proof,
            vec![
                new_data_account
                    .to_account_info()
                    .map_err(ProgramError::from)?,
                updated_data_account
                    .to_account_info()
                    .map_err(ProgramError::from)?,
            ],
            vec![new_account
                .address_tree_info
                .into_new_address_params_packed(new_address_seed)],
        );

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        msg!(
            "Created new account with message: '{}' and updated existing account to: '{}'",
            new_account.message,
            existing_account.update_message
        );

        Ok(())
    }
}

#[derive(Accounts)]
pub struct GenericAnchorAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(
    Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct DataAccount {
    #[hash]
    pub owner: Pubkey,
    #[hash]
    pub message: String,
}

#[derive(Clone, Debug, AnchorSerialize, AnchorDeserialize)]
pub struct ExistingCompressedAccountIxData {
    pub account_meta: CompressedAccountMeta,
    pub message: String,
    pub update_message: String,
}

#[derive(Clone, Debug, AnchorSerialize, AnchorDeserialize)]
pub struct NewCompressedAccountIxData {
    pub address_tree_info: PackedAddressTreeInfo,
    pub message: String,
}

#[error_code]
pub enum CustomError {
    #[msg("No authority to perform this action")]
    Unauthorized,
}
