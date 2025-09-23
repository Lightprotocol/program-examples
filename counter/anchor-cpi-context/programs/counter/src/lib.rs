#![allow(unexpected_cfgs)]

use anchor_lang::{prelude::*, AnchorDeserialize, Discriminator};
use light_sdk::{
    account::LightAccount,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    instruction::{
        account_meta::{CompressedAccountMeta, CompressedAccountMetaClose},
        PackedAddressTreeInfo, ValidityProof,
    },
    LightDiscriminator, LightHasher,
};

declare_id!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("GRLu2hKaAiMbxpkAM1HeXzks9YeGuz18SEgXEizVvPqX");

#[program]
pub mod counter {

    use light_batched_merkle_tree::queue::BatchedQueueAccount;
    use light_compressed_account::{
        address::derive_address,
        compressed_account::{CompressedAccount, CompressedAccountData},
        instruction_data::{
            cpi_context::CompressedCpiContext,
            data::OutputCompressedAccountWithPackedContext,
            with_readonly::{InAccount, InstructionDataInvokeCpiWithReadOnly},
        },
    };
    use light_hasher::{
        hash_to_field_size::hashv_to_bn254_field_size_be_const_array, DataHasher, Poseidon,
    };
    use light_sdk::{
        cpi::invoke_light_system_program, instruction::PackedMerkleContext,
        light_account_checks::AccountInfoTrait,
    };
    use light_sdk_types::{
        cpi_context_write::CpiContextWriteAccounts, CpiAccountsConfig, LIGHT_SYSTEM_PROGRAM_ID,
    };

    use super::*;

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
        let cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let seed = hashv_to_bn254_field_size_be_const_array::<3>(&[
            b"counter".as_slice(),
            ctx.accounts.signer.pubkey().as_ref(),
        ])
        .unwrap();
        msg!("seed {:?}", seed);
        msg!(
            "cpi_accounts.tree_pubkeys().unwrap()
            [address_tree_info.address_merkle_tree_pubkey_index as usize]
            .to_bytes() {:?}",
            cpi_accounts.tree_pubkeys().unwrap()
                [address_tree_info.address_merkle_tree_pubkey_index as usize]
                .to_bytes()
        );
        let address = derive_address(
            &seed,
            &cpi_accounts.tree_pubkeys().unwrap()
                [address_tree_info.address_merkle_tree_pubkey_index as usize]
                .to_bytes(),
            &crate::ID.to_bytes(),
        );
        msg!("address {:?}", address);

        let new_address_params = address_tree_info.into_new_address_params_packed(seed);
        msg!("new_address_params {:?}", new_address_params);

        let mut counter = LightAccount::<'_, CounterAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        counter.owner = ctx.accounts.signer.key();
        counter.value = 0;

        let cpi = CpiInputs::new_with_address(
            proof,
            vec![counter.to_account_info().map_err(ProgramError::from)?],
            vec![new_address_params],
        );
        cpi.invoke_light_system_program(cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }

    pub fn increment_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        let mut account_meta = account_meta;
        let light_cpi_accounts = CpiAccounts::new_with_config(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            CpiAccountsConfig {
                cpi_context: true,
                cpi_signer: crate::LIGHT_CPI_SIGNER,
                sol_compression_recipient: false,
                sol_pool_pda: false,
            },
        );
        // LightAccount::new_mut will create an account with input state and output state.
        // The input state is hashed immediately when calling new_mut().
        // Modifying the account will modify the output state that when converted to_account_info()
        // is hashed with poseidon hashes, serialized with borsh
        // and created with invoke_light_system_program by invoking the light-system-program.
        // The hashing scheme is the account structure derived with LightHasher.

        {
            let counter = CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            };

            msg!("invoke");
            let cpi_context_accounts = CpiContextWriteAccounts {
                fee_payer: light_cpi_accounts.fee_payer(),
                authority: light_cpi_accounts.authority().unwrap(),
                cpi_context: light_cpi_accounts.cpi_context().unwrap(),
                cpi_signer: LIGHT_CPI_SIGNER,
            };
            let data_hash = counter.hash::<Poseidon>().map_err(ProgramError::from)?;
            let instruction = InstructionDataInvokeCpiWithReadOnly {
                mode: 1u8,
                bump: LIGHT_CPI_SIGNER.bump,
                invoking_program_id: LIGHT_CPI_SIGNER.program_id.into(),
                compress_or_decompress_lamports: 0,
                is_compress: false,
                with_cpi_context: true,
                with_transaction_hash: false,
                cpi_context: CompressedCpiContext {
                    set_context: false,
                    first_set_context: true,
                    ..Default::default()
                },
                proof: None,
                new_address_params: vec![],
                input_compressed_accounts: vec![InAccount {
                    address: Some(account_meta.address),
                    discriminator: CounterAccount::discriminator(),
                    data_hash,
                    merkle_context: PackedMerkleContext {
                        merkle_tree_pubkey_index: account_meta.tree_info.merkle_tree_pubkey_index,
                        queue_pubkey_index: account_meta.tree_info.queue_pubkey_index,
                        prove_by_index: account_meta.tree_info.prove_by_index,
                        leaf_index: account_meta.tree_info.leaf_index,
                    },
                    root_index: account_meta.tree_info.root_index,
                    lamports: 0,
                }],
                output_compressed_accounts: vec![OutputCompressedAccountWithPackedContext {
                    compressed_account: CompressedAccount {
                        owner: LIGHT_CPI_SIGNER.program_id.into(),
                        lamports: 0,
                        address: Some(account_meta.address),
                        data: Some(CompressedAccountData {
                            data: vec![],
                            data_hash,
                            discriminator: CounterAccount::discriminator(),
                        }),
                    },
                    merkle_tree_index: account_meta.output_state_tree_index,
                }],
                read_only_addresses: vec![],
                read_only_accounts: vec![],
            };
            let inputs = instruction.try_to_vec().unwrap();
            let mut data = Vec::with_capacity(8 + inputs.len());
            data.extend_from_slice(
                &light_compressed_account::discriminators::DISCRIMINATOR_INVOKE_CPI_WITH_READ_ONLY,
            );
            data.extend(inputs);
            let instruction = anchor_lang::solana_program::instruction::Instruction {
                program_id: LIGHT_SYSTEM_PROGRAM_ID.into(),
                accounts: vec![
                    AccountMeta {
                        pubkey: light_cpi_accounts.fee_payer().pubkey(),
                        is_writable: light_cpi_accounts.fee_payer().is_writable,
                        is_signer: true,
                    },
                    AccountMeta {
                        pubkey: light_cpi_accounts.authority().unwrap().pubkey(),
                        is_writable: light_cpi_accounts.authority().unwrap().is_writable,
                        is_signer: true,
                    },
                    AccountMeta {
                        pubkey: light_cpi_accounts.cpi_context().unwrap().pubkey(),
                        is_writable: true,
                        is_signer: false,
                    },
                ],
                data,
            };
            invoke_light_system_program(
                &cpi_context_accounts.to_account_infos(),
                instruction,
                LIGHT_CPI_SIGNER.bump,
            )
            .map_err(ProgramError::from)?;
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
        )
        .map_err(ProgramError::from)?;

        let cpi_inputs = CpiInputs {
            proof,
            account_infos: Some(vec![counter
                .to_account_info()
                .map_err(ProgramError::from)?]),
            new_assigned_addresses: None,
            cpi_context: Some(CompressedCpiContext {
                set_context: false,
                first_set_context: false,
                cpi_context_account_index: 0,
            }),
            ..Default::default()
        };
        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;
        Ok(())
    }

    pub fn decrement_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

        counter.value = counter.value.checked_sub(1).ok_or(CustomError::Underflow)?;

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

    pub fn reset_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        let mut counter = LightAccount::<'_, CounterAccount>::new_mut(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

        counter.value = 0;

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

    pub fn close_counter<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        counter_value: u64,
        account_meta: CompressedAccountMetaClose,
    ) -> Result<()> {
        // LightAccount::new_close() will create an account with only input state and no output state.
        // By providing no output state the account is closed after the instruction.
        // The address of a closed account cannot be reused.
        let counter = LightAccount::<'_, CounterAccount>::new_close(
            &crate::ID,
            &account_meta,
            CounterAccount {
                owner: ctx.accounts.signer.key(),
                value: counter_value,
            },
        )
        .map_err(ProgramError::from)?;

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
