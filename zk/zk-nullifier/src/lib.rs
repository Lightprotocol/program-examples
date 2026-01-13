#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_sdk::account::LightAccount;
use light_sdk::cpi::v1::CpiAccounts;
use light_sdk::{
    address::v2::derive_address,
    cpi::{v1::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
    derive_light_cpi_signer,
    instruction::{CompressedProof, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator,
};
use light_sdk_types::CpiSigner;

declare_id!("NuL1fiErPRoCxidvVji4t8T5XvZBBdN5w1GWYxPxpJk");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("NuL1fiErPRoCxidvVji4t8T5XvZBBdN5w1GWYxPxpJk");

pub const NULLIFIER_PREFIX: &[u8] = b"nullifier";

// Max nullifiers per tx: 1 (single) or 4 (batch)
pub const BATCH_SIZE: usize = 4;

pub mod nullifier_1;
pub mod nullifier_batch_4;

#[program]
pub mod zk_nullifier {
    use groth16_solana::decompression::{decompress_g1, decompress_g2};

    use super::*;

    /// Creates 1 nullifier
    pub fn create_nullifier<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateNullifierAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        zk_proof: CompressedProof,
        verification_id: [u8; 32],
        nullifier: [u8; 32],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            return Err(ProgramError::InvalidAccountData.into());
        }

        let public_inputs: [[u8; 32]; 2] = [verification_id, nullifier];

        let proof_a = decompress_g1(&zk_proof.a).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let proof_b = decompress_g2(&zk_proof.b).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let proof_c = decompress_g1(&zk_proof.c).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let mut verifier = Groth16Verifier::new(
            &proof_a,
            &proof_b,
            &proof_c,
            &public_inputs,
            &crate::nullifier_1::VERIFYINGKEY,
        )
        .map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        verifier.verify().map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let (address, address_seed) = derive_address(
            &[
                NULLIFIER_PREFIX,
                nullifier.as_slice(),
                verification_id.as_slice(),
            ],
            &address_tree_pubkey,
            &crate::ID,
        );

        let nullifier_account = LightAccount::<NullifierAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(nullifier_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Creates 4 nullifiers with single proof
    pub fn create_batch_nullifier<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateNullifierAccounts<'info>>,
        proof: ValidityProof,
        address_tree_infos: [PackedAddressTreeInfo; BATCH_SIZE],
        output_state_tree_index: u8,
        zk_proof: CompressedProof,
        verification_id: [u8; 32],
        nullifiers: [[u8; 32]; BATCH_SIZE],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_infos[0]
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            return Err(ProgramError::InvalidAccountData.into());
        }

        // 5 public inputs: verification_id + 4 nullifiers
        let public_inputs: [[u8; 32]; 5] = [
            verification_id,
            nullifiers[0],
            nullifiers[1],
            nullifiers[2],
            nullifiers[3],
        ];

        let proof_a = decompress_g1(&zk_proof.a).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let proof_b = decompress_g2(&zk_proof.b).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let proof_c = decompress_g1(&zk_proof.c).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        let mut verifier = Groth16Verifier::new(
            &proof_a,
            &proof_b,
            &proof_c,
            &public_inputs,
            &crate::nullifier_batch_4::VERIFYINGKEY,
        )
        .map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        verifier.verify().map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        // Create 4 nullifier accounts
        let mut cpi_builder = LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof);
        let mut new_address_params = Vec::with_capacity(BATCH_SIZE);

        for i in 0..BATCH_SIZE {
            let (address, address_seed) = derive_address(
                &[
                    NULLIFIER_PREFIX,
                    nullifiers[i].as_slice(),
                    verification_id.as_slice(),
                ],
                &address_tree_pubkey,
                &crate::ID,
            );

            let nullifier_account = LightAccount::<NullifierAccount>::new_init(
                &crate::ID,
                Some(address),
                output_state_tree_index,
            );

            cpi_builder = cpi_builder.with_light_account(nullifier_account)?;
            new_address_params
                .push(address_tree_infos[i].into_new_address_params_packed(address_seed));
        }

        cpi_builder
            .with_new_addresses(&new_address_params)
            .invoke(light_cpi_accounts)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateNullifierAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct NullifierAccount {}

#[error_code]
pub enum ErrorCode {
    #[msg("Not enough keys in remaining accounts")]
    AccountNotEnoughKeys,
}
