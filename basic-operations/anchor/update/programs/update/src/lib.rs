#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{v1::CpiAccounts, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator,
};

declare_id!("Cj3DxyqB7wJh511VKexsjKt7Hx1kvPvCBMrbLuL8grKc");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("Cj3DxyqB7wJh511VKexsjKt7Hx1kvPvCBMrbLuL8grKc");

#[program]
pub mod update {

    use super::*;
    use light_sdk::cpi::{
        v1::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction,
    };

    /// Setup: Creates a compressed account
    pub fn create_account<'info>(
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
            &[b"message", ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut my_compressed_account = LightAccount::<MyCompressedAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        my_compressed_account.owner = ctx.accounts.signer.key();
        my_compressed_account.message = message.clone();

        msg!(
            "Created compressed account with message: {}",
            my_compressed_account.message
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(my_compressed_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Updates an existing compressed account's message
    pub fn update_account<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        current_account: MyCompressedAccount,
        account_meta: CompressedAccountMeta,
        new_message: String,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let mut my_compressed_account = LightAccount::<MyCompressedAccount>::new_mut(
            &crate::ID,
            &account_meta,
            current_account,
        )?;

        my_compressed_account.message = new_message.clone();

        msg!(
            "Updated compressed account message to: {}",
            my_compressed_account.message
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(my_compressed_account)?
            .invoke(light_cpi_accounts)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct GenericAnchorAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[event]
#[derive(Clone, Debug, Default, LightDiscriminator)]
pub struct MyCompressedAccount {
    pub owner: Pubkey,
    pub message: String,
}