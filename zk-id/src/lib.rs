#![allow(unexpected_cfgs)]

use account_compression::{state_merkle_tree_from_bytes_zero_copy, StateMerkleTreeAccount};
use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_compressed_account::instruction_data::compressed_proof::CompressedProof;
use light_sdk::account::{poseidon::LightAccount as LightAccountPoseidon, LightAccount};
use light_sdk::cpi::v1::CpiAccounts;
use light_sdk::{
    address::v1::derive_address,
    cpi::{v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator, LightHasher,
};
use light_sdk_types::CpiSigner;

declare_id!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

pub const ISSUER: &[u8] = b"issuer";
pub const ZK_ID_CHECK: &[u8] = b"ZK_ID_CHECK";

// Include the generated verifying key module
pub mod verifying_key;

#[program]
pub mod zk_id {

    use groth16_solana::decompression::{decompress_g1, decompress_g2};
    use light_hasher::hash_to_field_size::{
        hashv_to_bn254_field_size_be_array, hashv_to_bn254_field_size_be_const_array,
    };
    use light_sdk::instruction::account_info::CompressedAccountInfoTrait;

    use super::*;

    /// Creates a new issuer compressed account
    pub fn create_issuer<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let (address, address_seed) = derive_address(
            &[ISSUER, ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut issuer_account = LightAccount::<'_, IssuerAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        issuer_account.issuer_pubkey = ctx.accounts.signer.key();
        issuer_account.num_credentials_issued = 0;

        msg!(
            "Created issuer account for pubkey: {}",
            ctx.accounts.signer.key()
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(issuer_account)?
            .with_new_addresses(&[
                address_tree_info.into_new_address_params_assigned_packed(address_seed, Some(0))
            ])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Creates a new credential compressed account storing a pubkey
    /// Requires a valid issuer account - only the issuer can create credentials
    pub fn add_credential<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        issuer_account_meta: CompressedAccountMeta,
        credential_pubkey: Pubkey,
        num_credentials_issued: u64,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Verify the issuer account - read it to ensure it exists and signer is the issuer
        let mut issuer_account = LightAccount::<'_, IssuerAccount>::new_mut(
            &crate::ID,
            &issuer_account_meta,
            IssuerAccount {
                issuer_pubkey: ctx.accounts.signer.key(),
                num_credentials_issued,
            },
        )?;

        // Increment the credential counter
        issuer_account.num_credentials_issued = issuer_account
            .num_credentials_issued
            .checked_add(1)
            .ok_or(ProgramError::ArithmeticOverflow)?;

        let (address, address_seed) = derive_address(
            &[ISSUER, ctx.accounts.signer.key().as_ref()],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::AccountNotEnoughKeys)?,
            &crate::ID,
        );

        let mut credential_account = LightAccountPoseidon::<'_, CredentialAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        credential_account.issuer = ctx.accounts.signer.key();
        credential_account.credential_pubkey = credential_pubkey;

        msg!(
            "Created credential account for pubkey: {} (issuer credential count: {})",
            credential_pubkey,
            issuer_account.num_credentials_issued
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(issuer_account)?
            .with_light_account_poseidon(credential_account)?
            .with_new_addresses(&[
                address_tree_info.into_new_address_params_assigned_packed(address_seed, Some(0))
            ])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Verifies a ZK proof of credential ownership and creates an encrypted event account.
    pub fn zk_verify_credential<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        input_root_index: u16,
        encrypted_data: Vec<u8>,
        credential_proof: CompressedProof,
        issuer: [u8; 32],
        merkle_tree_hashed: [u8; 32],
        data_hash: [u8; 32],
        verification_id: [u8; 32],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );
        let address_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V1 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let (address, address_seed) = derive_address(
            &[
                ZK_ID_CHECK,
                data_hash.as_slice(),
                verification_id.as_slice(),
            ],
            &address_pubkey,
            &crate::ID,
        );

        // Get root from input Merkle tree (example of reading on-chain state)
        let expected_root =
            read_merkle_tree_root(&ctx.accounts.input_merkle_tree, input_root_index)?;
        let mut discriminator = [0u8; 32];
        discriminator[24..].copy_from_slice(CredentialAccount::LIGHT_DISCRIMINATOR_SLICE);
        let issuer_hashed = hashv_to_bn254_field_size_be_const_array::<2>(&[&issuer]).unwrap();
        let account_owner_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&crate::ID.to_bytes()]).unwrap();

        let mut event_account = LightAccount::<'_, EncryptedEventAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );
        event_account.data = encrypted_data;
        let event_account_info = event_account.to_account_info()?;

        // Construct public inputs array for the circuit
        // Order must match the circuit: owner_hashed, merkle_tree_hashed, discriminator, issuer_hashed, expectedRoot
        let public_inputs: [[u8; 32]; 7] = [
            account_owner_hashed,
            merkle_tree_hashed,
            discriminator,
            issuer_hashed,
            expected_root,
            event_account_info.output.as_ref().unwrap().data_hash,
            data_hash,
        ];
        let proof_a = decompress_g1(&credential_proof.a).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;
        let proof_b = decompress_g2(&credential_proof.b).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;
        let proof_c = decompress_g1(&credential_proof.c).map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        // Verify the Groth16 proof
        let mut verifier = Groth16Verifier::new(
            &proof_a,
            &proof_b,
            &proof_c,
            &public_inputs,
            &crate::verifying_key::VERIFYINGKEY,
        )
        .map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        verifier.verify().map_err(|e| {
            let code: u32 = e.into();
            Error::from(ProgramError::Custom(code))
        })?;

        msg!("ZK proof verified successfully");

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_account_infos(&[event_account_info])
            .with_new_addresses(&[
                address_tree_info.into_new_address_params_assigned_packed(address_seed, Some(0))
            ])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct GenericAnchorAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    pub input_merkle_tree: AccountLoader<'info, StateMerkleTreeAccount>,
}

#[derive(
    Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct CredentialAccount {
    #[hash]
    pub issuer: Pubkey,
    #[hash]
    pub credential_pubkey: Pubkey,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct EncryptedEventAccount {
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct IssuerAccount {
    pub issuer_pubkey: Pubkey,
    pub num_credentials_issued: u64,
}

/// Reads a root from the concurrent state merkle tree by index
pub fn read_merkle_tree_root(
    input_merkle_tree: &AccountLoader<StateMerkleTreeAccount>,
    root_index: u16,
) -> Result<[u8; 32]> {
    let account_info = input_merkle_tree.to_account_info();
    let account_data = account_info.try_borrow_data()?;

    let merkle_tree = state_merkle_tree_from_bytes_zero_copy(&account_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if root_index as usize >= merkle_tree.roots.len() {
        return Err(ProgramError::InvalidArgument.into());
    }

    Ok(merkle_tree.roots[root_index as usize])
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid issuer: signer is not the issuer of this account")]
    InvalidIssuer,
    #[msg("Not enough keys in remaining accounts")]
    AccountNotEnoughKeys,
}
