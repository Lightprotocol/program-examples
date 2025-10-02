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

#[program]
pub mod read_only {

    use light_compressed_account::{
        compressed_account::{
            CompressedAccount, CompressedAccountData, PackedReadOnlyCompressedAccount,
        },
        instruction_data::with_readonly::InstructionDataInvokeCpiWithReadOnly,
    };
    use light_hasher::{DataHasher, Poseidon};
    use light_sdk::cpi::{invoke_light_system_program, to_account_metas};
    use light_sdk_types::LIGHT_SYSTEM_PROGRAM_ID;

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
    pub fn read<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        existing_account: ExistingCompressedAccountIxData,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let read_data_account = DataAccount {
            owner: ctx.accounts.signer.key(),
            message: existing_account.message.clone(),
        };
        let compressed_account = CompressedAccount {
            address: Some(existing_account.account_meta.address),
            owner: crate::ID.to_bytes().into(),
            data: Some(CompressedAccountData {
                data: vec![], // not used to compute the hash
                data_hash: read_data_account.hash::<Poseidon>().unwrap(),
                discriminator: DataAccount::discriminator(),
            }),
            lamports: 0,
        };
        let merkle_tree_pubkey = light_cpi_accounts
            .get_tree_account_info(
                existing_account
                    .account_meta
                    .tree_info
                    .merkle_tree_pubkey_index as usize,
            )
            .unwrap()
            .key
            .to_bytes()
            .into();
        let account_hash = compressed_account
            .hash(
                &merkle_tree_pubkey,
                &existing_account.account_meta.tree_info.leaf_index,
                true,
            )
            .unwrap();

        let instruction_data = InstructionDataInvokeCpiWithReadOnly {
            read_only_accounts: vec![PackedReadOnlyCompressedAccount {
                root_index: existing_account.account_meta.tree_info.root_index,
                merkle_context: light_sdk::instruction::PackedMerkleContext {
                    merkle_tree_pubkey_index: existing_account
                        .account_meta
                        .tree_info
                        .merkle_tree_pubkey_index,
                    queue_pubkey_index: existing_account.account_meta.tree_info.queue_pubkey_index,
                    leaf_index: existing_account.account_meta.tree_info.leaf_index,
                    prove_by_index: existing_account.account_meta.tree_info.prove_by_index,
                },
                account_hash,
            }],
            proof: proof.into(),
            bump: LIGHT_CPI_SIGNER.bump,
            invoking_program_id: LIGHT_CPI_SIGNER.program_id.into(),
            mode: 0,
            ..Default::default()
        };
        let inputs = instruction_data.try_to_vec().unwrap();

        let mut data = Vec::with_capacity(8 + inputs.len());
        data.extend_from_slice(
            &light_compressed_account::discriminators::DISCRIMINATOR_INVOKE_CPI_WITH_READ_ONLY,
        );
        data.extend(inputs);
        let account_infos = light_cpi_accounts
            .to_account_infos()
            .iter()
            .map(|e| e.to_account_info())
            .collect::<Vec<_>>();
        let account_metas: Vec<AccountMeta> = to_account_metas(light_cpi_accounts).unwrap();
        let instruction = anchor_lang::solana_program::instruction::Instruction {
            accounts: account_metas,
            data,
            program_id: LIGHT_SYSTEM_PROGRAM_ID.into(),
        };
        invoke_light_system_program(account_infos.as_slice(), instruction, LIGHT_CPI_SIGNER.bump)
            .map_err(ProgramError::from)?;

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
}
