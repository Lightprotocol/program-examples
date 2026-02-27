#![allow(deprecated)]

use anchor_lang::prelude::*;
use light_sdk::{
    account::LightAccount,
    address::v2::derive_address,
    cpi::{v2::CpiAccounts, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher, PackedAddressTreeInfoExt,
};
use light_sdk::constants::ADDRESS_TREE_V2;

#[error_code]
pub enum CustomError {
    #[msg("No authority to perform this action")]
    Unauthorized,
}

declare_id!("FYX4GmKJYzSiycc7XZKf12NGXNE9siSx1cJubYJniHcv");

const CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("FYX4GmKJYzSiycc7XZKf12NGXNE9siSx1cJubYJniHcv");

#[program]
pub mod account_comparison {
    use light_sdk::cpi::{
        v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction,
    };
    use light_sdk::error::LightSdkError;

    use super::*;

    pub fn create_account(ctx: Context<CreateAccount>, name: String) -> Result<()> {
        let account = &mut ctx.accounts.account;
        account.data = [1; 128];
        account.name = name;
        account.user = *ctx.accounts.user.key;

        Ok(())
    }

    pub fn update_data(ctx: Context<UpdateData>, data: [u8; 128]) -> Result<()> {
        let account = &mut ctx.accounts.account;
        account.data = data;
        Ok(())
    }

    pub fn create_compressed_account<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateCompressedAccount<'info>>,
        name: String,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_tree_index: u8,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.user.as_ref(),
            ctx.remaining_accounts,
            CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|err| ProgramError::from(LightSdkError::from(err)))?;

        if address_tree_pubkey.to_bytes() != ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let (address, address_seed) = derive_address(
            &[b"account", ctx.accounts.user.key().as_ref()],
            &address_tree_pubkey,
            &crate::ID,
        );

        // LightAccount::new_init will create an account with empty output state (no input state).
        // Modifying the account will modify the output state that when converted to_account_info()
        // is hashed with poseidon hashes, serialized with borsh
        // and created with invoke_light_system_program by invoking the light-system-program.
        // The hashing scheme is the account structure derived with LightHasher.
        let mut compressed_account = LightAccount::<CompressedAccountData>::new_init(
            &crate::ID,
            Some(address),
            output_tree_index,
        );

        compressed_account.user = ctx.accounts.user.key();
        compressed_account.name = name;
        compressed_account.data = [1u8; 128];

        let new_address_params =
            address_tree_info.into_new_address_params_assigned_packed(address_seed, Some(0));

        LightSystemProgramCpi::new_cpi(CPI_SIGNER, proof)
            .with_light_account(compressed_account)?
            .with_new_addresses(&[new_address_params])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    pub fn update_compressed_account<'info>(
        ctx: Context<'_, '_, '_, 'info, UpdateCompressedAccount<'info>>,
        new_data: [u8; 128],
        existing_data: [u8; 128],
        name: String,
        proof: ValidityProof,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        let mut compressed_account = LightAccount::<CompressedAccountData>::new_mut(
            &crate::ID,
            &account_meta,
            CompressedAccountData {
                user: ctx.accounts.user.key(),
                data: existing_data,
                name,
            },
        )?;

        if compressed_account.user != ctx.accounts.user.key() {
            return err!(CustomError::Unauthorized);
        }

        compressed_account.data = new_data;

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.user.as_ref(),
            ctx.remaining_accounts,
            CPI_SIGNER,
        );

        LightSystemProgramCpi::new_cpi(CPI_SIGNER, proof)
            .with_light_account(compressed_account)?
            .invoke(light_cpi_accounts)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateAccount<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(init, payer = user, space = 8 + 32 + 128 + 64, seeds = [b"account", user.key().as_ref()], bump)]
    pub account: Account<'info, AccountData>,
    pub system_program: Program<'info, System>,
}

/// [0..8, 8..40,40..168,168..232]
#[account]
#[derive(Debug)]
pub struct AccountData {
    pub user: Pubkey,
    pub name: String,
    pub data: [u8; 128],
}

#[derive(Accounts)]
pub struct UpdateData<'info> {
    #[account(mut, has_one = user)]
    pub account: Account<'info, AccountData>,
    #[account(mut)]
    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct CreateCompressedAccount<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateCompressedAccount<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
}

#[derive(Clone, Debug, AnchorDeserialize, AnchorSerialize, LightDiscriminator, LightHasher)]
pub struct CompressedAccountData {
    #[hash]
    pub user: Pubkey,
    #[hash]
    pub name: String,
    #[hash]
    pub data: [u8; 128],
}

impl Default for CompressedAccountData {
    fn default() -> Self {
        Self {
            user: Pubkey::default(),
            name: String::default(),
            data: [0u8; 128],
        }
    }
}
