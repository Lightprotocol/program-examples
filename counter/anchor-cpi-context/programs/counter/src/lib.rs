// suppress target_os = "solana" cfg warnings
#![allow(unexpected_cfgs)]
// anchor program macro uses deprecated resize
#![allow(deprecated)]

use anchor_lang::{prelude::*, AnchorDeserialize, Discriminator};
use light_batched_merkle_tree::queue::BatchedQueueAccount;
use light_compressed_account::instruction_data::with_readonly::InstructionDataInvokeCpiWithReadOnly;
use light_sdk::address::v2::derive_address;
use light_sdk::cpi::{InvokeLightSystemProgram, LightCpiInstruction};
use light_sdk::{
    account::LightAccount,
    cpi::{CpiAccountsV2, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher,
};
use light_sdk_types::{cpi_context_write::CpiContextWriteAccounts, CpiAccountsConfig};

declare_id!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

#[program]
pub mod counter {

    use super::*;
    use light_sdk::light_account_checks::AccountInfoTrait;

    pub fn create_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
    ) -> Result<()> {
        // LightAccount::new_init will create an account with empty output state (no input state).
        // Modifying the account will modify the output state that when converted to_account_info()
        // is hashed with poseidon hashes, serialized with borsh
        // and created with invoke_light_system_program by invoking the light-system-program.
        // The hashing scheme is the account structure derived with LightHasher.
        let cpi_accounts = CpiAccountsV2::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let (address, seed) = derive_address(
            &[b"counter".as_slice(), ctx.accounts.signer.pubkey().as_ref()],
            &cpi_accounts.tree_pubkeys().unwrap()
                [address_tree_info.address_merkle_tree_pubkey_index as usize],
            &crate::ID,
        );

        let new_address_params =
            address_tree_info.into_new_address_params_assigned_packed(seed.into(), Some(0));

        let mut counter = LightAccount::<'_, CounterAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        counter.owner = ctx.accounts.signer.key();
        counter.value = 0;

        InstructionDataInvokeCpiWithReadOnly::new_cpi(LIGHT_CPI_SIGNER, proof)
            .mode_v2()
            .with_light_account(counter)?
            .with_new_addresses(&[new_address_params])
            .invoke(cpi_accounts)?;
        Ok(())
    }

    pub fn change_owner_with_cpi_context<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        let mut account_meta = account_meta;
        let fee_payer = ctx.accounts.signer.to_account_info();
        let light_cpi_accounts = CpiAccountsV2::new_with_config(
            &fee_payer,
            ctx.remaining_accounts,
            CpiAccountsConfig {
                cpi_context: true,
                cpi_signer: crate::LIGHT_CPI_SIGNER,
                sol_compression_recipient: false,
                sol_pool_pda: false,
            },
        );

        {
            let cpi_context_accounts =
                CpiContextWriteAccounts::<AccountInfo>::try_from(&light_cpi_accounts).unwrap();

            let counter = LightAccount::<'_, CounterAccount>::new_mut(
                &crate::ID,
                &account_meta,
                CounterAccount {
                    owner: ctx.accounts.signer.key(),
                    value: counter_value,
                },
            )?;

            let in_account = counter
                .to_in_account()
                .ok_or(ProgramError::InvalidAccountData)?;
            let out_account = counter
                .to_output_compressed_account_with_packed_context(Some(
                    LIGHT_CPI_SIGNER.program_id.into(),
                ))?
                .ok_or(ProgramError::InvalidAccountData)?;
            InstructionDataInvokeCpiWithReadOnly::new_cpi(LIGHT_CPI_SIGNER, None.into())
                .mode_v2()
                .with_input_compressed_accounts(&[in_account])
                .with_output_compressed_accounts(&[out_account])
                .invoke_write_to_cpi_context_first(cpi_context_accounts)?;
        }
        msg!(
            "tree pubkeys {:?} ",
            light_cpi_accounts.tree_pubkeys().unwrap()
        );
        let account_info = light_cpi_accounts.get_tree_account_info(1).unwrap();

        let output_queue = BatchedQueueAccount::output_from_account_info(account_info).unwrap();
        account_meta.tree_info.leaf_index = output_queue.batch_metadata.next_index as u32;
        account_meta.tree_info.prove_by_index = true;
        let counter = LightAccount::<'_, CounterAccount>::new_mut(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )?;

        InstructionDataInvokeCpiWithReadOnly::new(
            LIGHT_CPI_SIGNER.program_id.into(),
            LIGHT_CPI_SIGNER.bump,
            proof.into(),
        )
        .mode_v2()
        .with_light_account(counter)?
        .invoke_execute_cpi_context(light_cpi_accounts)?;
        Ok(())
    }
}

#[error_code]
pub enum CustomError {
    #[msg("No authority to perform this action")]
    Unauthorized,
    #[msg("Counter overflow")]
    Overflow,
    #[msg("Counter underflow")]
    Underflow,
}

#[derive(Accounts)]
pub struct GenericAnchorAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

// declared as event so that it is part of the idl.
#[event]
#[derive(Clone, Debug, Default, LightDiscriminator, LightHasher)]
pub struct CounterAccount {
    #[hash]
    pub owner: Pubkey,
    pub value: u64,
}
