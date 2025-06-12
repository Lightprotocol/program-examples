use anchor_lang::prelude::*;
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs},
    instruction::{account_meta::CompressedAccountMeta, tree_info::PackedAddressTreeInfo},
    LightDiscriminator, LightHasher, ValidityProof,
};

#[error_code]
pub enum CustomError {
    #[msg("No authority to perform this action")]
    Unauthorized,
}

declare_id!("FYX4GmKJYzSiycc7XZKf12NGXNE9siSx1cJubYJniHcv");

#[program]
pub mod account_comparison {
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
        let program_id = crate::ID.into();
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.user.as_ref(),
            ctx.remaining_accounts,
            crate::ID,
        )
        .map_err(ProgramError::from)?;

        let (address, address_seed) = derive_address(
            &[b"account", ctx.accounts.user.key().as_ref()],
            &light_cpi_accounts.tree_accounts()
                [address_tree_info.address_merkle_tree_pubkey_index as usize]
                .key(),
            &crate::ID,
        );

        // LightAccount::new_init will create an account with empty output state (no input state).
        // Modifying the account will modify the output state that when converted to_account_info()
        // is hashed with poseidon hashes, serialized with borsh
        // and created with invoke_light_system_program by invoking the light-system-program.
        // The hashing scheme is the account structure derived with LightHasher.
        let mut compressed_account = LightAccount::<'_, CompressedAccountData>::new_init(
            &program_id,
            Some(address),
            output_tree_index,
        );

        compressed_account.user = ctx.accounts.user.key();
        compressed_account.name = name;
        compressed_account.data = [1u8; 128];

        let new_address_params = address_tree_info.into_new_address_params_packed(address_seed);

        let cpi = CpiInputs::new_with_address(
            proof,
            vec![compressed_account
                .to_account_info()
                .map_err(ProgramError::from)?],
            vec![new_address_params],
        );
        cpi.invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

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
        let program_id = crate::ID.into();
        let mut compressed_account = LightAccount::<'_, CompressedAccountData>::new_mut(
            &program_id,
            &account_meta,
            CompressedAccountData {
                user: ctx.accounts.user.key(),
                data: existing_data,
                name,
            },
        )
        .map_err(ProgramError::from)?;

        if compressed_account.user != ctx.accounts.user.key() {
            return err!(CustomError::Unauthorized);
        }

        compressed_account.data = new_data;

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.user.as_ref(),
            ctx.remaining_accounts,
            crate::ID,
        )
        .map_err(ProgramError::from)?;

        let cpi_inputs = CpiInputs::new(
            proof,
            vec![compressed_account
                .to_account_info()
                .map_err(ProgramError::from)?],
        );

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

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
