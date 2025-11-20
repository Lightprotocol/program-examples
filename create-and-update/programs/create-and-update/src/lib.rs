#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use light_sdk::{
    account::LightAccount,
    address::v2::derive_address,
    cpi::{v2::CpiAccounts, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator,
};

declare_id!("J6K7nvoVpJHfH13zn47vptnZo1JdUGCGSiVmtfkzz9NA");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("J6K7nvoVpJHfH13zn47vptnZo1JdUGCGSiVmtfkzz9NA");

pub const FIRST_SEED: &[u8] = b"first";
pub const SECOND_SEED: &[u8] = b"second";

#[program]
pub mod create_and_update {

    use super::*;
    use light_sdk::cpi::{
        v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction,
    };

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

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;
        let (address, address_seed) = derive_address(
            &[FIRST_SEED, ctx.accounts.signer.key().as_ref()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut data_account = LightAccount::<DataAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        data_account.owner = ctx.accounts.signer.key();
        data_account.message = message;

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(data_account)?
            .with_new_addresses(&[
                address_tree_info.into_new_address_params_assigned_packed(address_seed, Some(0))
            ])
            .invoke(light_cpi_accounts)?;

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

        let new_account_address_tree_pubkey = &new_account
            .address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        // Create new compressed account
        let (new_address, new_address_seed) = derive_address(
            &[SECOND_SEED, ctx.accounts.signer.key().as_ref()],
            new_account_address_tree_pubkey,
            &crate::ID,
        );

        let mut new_data_account = LightAccount::<DataAccount>::new_init(
            &crate::ID,
            Some(new_address),
            existing_account.account_meta.output_state_tree_index,
        );
        new_data_account.owner = ctx.accounts.signer.key();
        new_data_account.message = new_account.message.clone();

        let mut updated_data_account = LightAccount::<DataAccount>::new_mut(
            &crate::ID,
            &existing_account.account_meta,
            DataAccount {
                owner: ctx.accounts.signer.key(),
                message: existing_account.message.clone(),
            },
        )?;

        // Update the message
        updated_data_account.message = existing_account.update_message.clone();

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(new_data_account)?
            .with_light_account(updated_data_account)?
            .with_new_addresses(&[new_account
                .address_tree_info
                .into_new_address_params_assigned_packed(new_address_seed, Some(0))])
            .invoke(light_cpi_accounts)?;

        msg!(
            "Created new account with message: '{}' and updated existing account to: '{}'",
            new_account.message,
            existing_account.update_message
        );

        Ok(())
    }

    /// Updates two existing compressed accounts in a single instruction
    pub fn update_two_accounts<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        first_account: ExistingCompressedAccountIxData,
        second_account: ExistingCompressedAccountIxData,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Update first compressed account
        let mut updated_first_account = LightAccount::<DataAccount>::new_mut(
            &crate::ID,
            &first_account.account_meta,
            DataAccount {
                owner: ctx.accounts.signer.key(),
                message: first_account.message.clone(),
            },
        )?;

        // Update the message of the first account
        updated_first_account.message = first_account.update_message.clone();

        // Update second compressed account
        let mut updated_second_account = LightAccount::<DataAccount>::new_mut(
            &crate::ID,
            &second_account.account_meta,
            DataAccount {
                owner: ctx.accounts.signer.key(),
                message: second_account.message.clone(),
            },
        )?;

        // Update the message of the second account
        updated_second_account.message = second_account.update_message.clone();

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(updated_first_account)?
            .with_light_account(updated_second_account)?
            .invoke(light_cpi_accounts)?;

        msg!(
            "Updated first account to: '{}' and second account to: '{}'",
            first_account.update_message,
            second_account.update_message
        );

        Ok(())
    }

    /// Creates two new compressed accounts with different addresses in a single instruction
    pub fn create_two_accounts<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        byte_data: [u8; 31],
        message: String,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Create first compressed account
        let (first_address, first_address_seed) = derive_address(
            &[FIRST_SEED, ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut first_data_account = LightAccount::<ByteDataAccount>::new_init(
            &crate::ID,
            Some(first_address),
            output_state_tree_index,
        );
        first_data_account.owner = ctx.accounts.signer.key();
        first_data_account.data = byte_data;

        // Create second compressed account
        let (second_address, second_address_seed) = derive_address(
            &[SECOND_SEED, ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut second_data_account = LightAccount::<DataAccount>::new_init(
            &crate::ID,
            Some(second_address),
            output_state_tree_index,
        );
        second_data_account.owner = ctx.accounts.signer.key();
        second_data_account.message = message.clone();

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(first_data_account)?
            .with_light_account(second_data_account)?
            .with_new_addresses(&[
                address_tree_info
                    .into_new_address_params_assigned_packed(first_address_seed, Some(0)),
                address_tree_info
                    .into_new_address_params_assigned_packed(second_address_seed, Some(1)),
            ])
            .invoke(light_cpi_accounts)?;

        msg!(
            "Created byte account with data: {:?} and string account with message: '{}'",
            byte_data,
            message
        );

        Ok(())
    }
}

#[derive(Accounts)]
pub struct GenericAnchorAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(Clone, LightDiscriminator, Default, AnchorDeserialize, AnchorSerialize)]
pub struct DataAccount {
    pub owner: Pubkey,
    pub message: String,
}

#[derive(Clone, LightDiscriminator, Default, AnchorDeserialize, AnchorSerialize)]
pub struct ByteDataAccount {
    pub owner: Pubkey,
    pub data: [u8; 31],
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

// for idl generation.
#[event]
pub struct AccountTypes {
    pub data_account: DataAccount,
    pub byte_data_account: ByteDataAccount,
}
