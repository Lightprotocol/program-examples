#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use nullifier_creation::{create_nullifiers, NullifierInstructionData};

declare_id!("Bw8aty8LJY5Kg2b6djghjWGwt6cBc1tVQUoreUehvVq4");

#[program]
pub mod nullifier {
    use super::*;

    /// Creates nullifier accounts for the provided nullifier values.
    pub fn create_nullifier<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateNullifierAccounts<'info>>,
        data: NullifierInstructionData,
        nullifiers: Vec<[u8; 32]>,
    ) -> Result<()> {
        // Verify your proof here. Use nullifiers as public inputs
        // among your other public inputs.
        // Example:
        // let public_inputs = [...nullifiers, ...your_other_inputs];
        // Groth16Verifier::new(...).verify()?;

        create_nullifiers(
            &nullifiers,
            data,
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
        )
    }
}

#[derive(Accounts)]
pub struct CreateNullifierAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

pub mod nullifier_creation {
    use super::*;
    use borsh::{BorshDeserialize, BorshSerialize};
    use light_sdk::account::LightAccount;
    use light_sdk::cpi::v2::CpiAccounts;
    use light_sdk::{
        address::{v2::derive_address, NewAddressParamsAssignedPacked},
        cpi::{v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
        derive_light_cpi_signer,
        instruction::{PackedAddressTreeInfo, ValidityProof},
        LightDiscriminator,
    };
    use light_sdk_types::CpiSigner;
    #[error_code]
    pub enum ErrorCode {
        #[msg("Not enough keys in remaining accounts")]
        AccountNotEnoughKeys,
    }

    #[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
    pub struct NullifierAccount {}

    pub const LIGHT_CPI_SIGNER: CpiSigner =
        derive_light_cpi_signer!("Bw8aty8LJY5Kg2b6djghjWGwt6cBc1tVQUoreUehvVq4");

    pub const NULLIFIER_PREFIX: &[u8] = b"nullifier";

    #[derive(Clone, Debug, AnchorSerialize, AnchorDeserialize)]
    pub struct NullifierInstructionData {
        pub proof: ValidityProof,
        pub address_tree_info: PackedAddressTreeInfo,
        pub output_state_tree_index: u8,
        pub system_accounts_offset: u8,
    }

    /// Creates nullifier compressed pdas for the given nullifier values.
    ///
    /// # Arguments
    /// * `nullifiers` - Slice of nullifier values to create compressed pdas for
    /// * `data` - Instruction data with proof and tree info
    /// * `remaining_accounts` - Remaining accounts must contain zk compression system program accounts and Merkle trees.
    pub fn create_nullifiers<'info>(
        nullifiers: &[[u8; 32]],
        data: NullifierInstructionData,
        signer: &AccountInfo<'info>,
        remaining_accounts: &[AccountInfo<'info>],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            signer,
            &remaining_accounts[data.system_accounts_offset as usize..],
            LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = data
            .address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let mut cpi_builder = LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, data.proof);
        let mut new_address_params: Vec<NewAddressParamsAssignedPacked> =
            Vec::with_capacity(nullifiers.len());

        for (i, nullifier) in nullifiers.iter().enumerate() {
            let (address, address_seed) = derive_address(
                &[NULLIFIER_PREFIX, nullifier.as_slice()],
                &address_tree_pubkey,
                &crate::ID,
            );

            let nullifier_account = LightAccount::<NullifierAccount>::new_init(
                &crate::ID,
                Some(address),
                data.output_state_tree_index,
            );

            cpi_builder = cpi_builder.with_light_account(nullifier_account)?;
            new_address_params.push(
                data.address_tree_info
                    .into_new_address_params_assigned_packed(address_seed, Some(i as u8)),
            );
        }

        cpi_builder
            .with_new_addresses(&new_address_params)
            .invoke(light_cpi_accounts)?;

        Ok(())
    }
}
