#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_hasher::to_byte_array::ToByteArray;
use light_hasher::{Hasher, HasherError, Sha256};
use light_sdk::account::{poseidon::LightAccount as LightAccountPoseidon, LightAccount};
use light_sdk::cpi::v2::CpiAccounts;
use light_sdk::{
    address::v2::derive_address,
    cpi::{v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
    derive_light_cpi_signer,
    instruction::{
        account_meta::CompressedAccountMeta, CompressedProof, PackedAddressTreeInfo, ValidityProof,
    },
    merkle_tree::v1::read_state_merkle_tree_root,
    LightDiscriminator, LightHasher,
};
use light_sdk_types::CpiSigner;

declare_id!("8HYAuAkoLp2UG4mgkqUcJBXo2bzaaKy8nBL62L4S3SSB");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("8HYAuAkoLp2UG4mgkqUcJBXo2bzaaKy8nBL62L4S3SSB");

pub const ISSUER: &[u8] = b"issuer";
pub const CREDENTIAL: &[u8] = b"credential";
pub const ZK_ID_CHECK: &[u8] = b"ZK_ID_CHECK";

// Include the generated verifying key module
pub mod verifying_key;

#[program]
pub mod zk_id {

    use groth16_solana::decompression::{decompress_g1, decompress_g2};
    use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;

    use super::*;

    /// Creates a new issuer compressed account
    pub fn create_issuer<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        system_accounts_offset: u8,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            &ctx.remaining_accounts[system_accounts_offset as usize..],
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let (address, address_seed) = derive_address(
            &[ISSUER, ctx.accounts.signer.key().as_ref()],
            &address_tree_pubkey,
            &crate::ID,
        );
        msg!("address {:?}", address);
        let mut issuer_account = LightAccount::<IssuerAccount>::new_init(
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
    #[allow(clippy::too_many_arguments)]
    pub fn add_credential<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        system_accounts_offset: u8,
        issuer_account_meta: CompressedAccountMeta,
        credential_pubkey: Pubkey,
        num_credentials_issued: u64,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            &ctx.remaining_accounts[system_accounts_offset as usize..],
            crate::LIGHT_CPI_SIGNER,
        );

        // Verify the issuer account - read it to ensure it exists and signer is the issuer
        let mut issuer_account = LightAccount::<IssuerAccount>::new_mut(
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

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let (address, address_seed) = derive_address(
            &[CREDENTIAL, credential_pubkey.as_ref()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut credential_account = LightAccountPoseidon::<CredentialAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        credential_account.issuer = ctx.accounts.signer.key();
        credential_account.credential_pubkey = CredentialPubkey::new(credential_pubkey);

        msg!(
            "Created credential account for pubkey: {} (issuer credential count: {})",
            credential_pubkey,
            issuer_account.num_credentials_issued
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(issuer_account)?
            .with_light_account_poseidon(credential_account)?
            .with_new_addresses(&[
                address_tree_info.into_new_address_params_assigned_packed(address_seed, Some(1))
            ])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Verifies a ZK proof of credential ownership and creates an encrypted event account.
    #[allow(clippy::too_many_arguments)]
    pub fn zk_verify_credential<'info>(
        ctx: Context<'_, '_, '_, 'info, VerifyAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        system_accounts_offset: u8,
        input_root_index: u16,
        public_data: Vec<u8>,
        credential_proof: CompressedProof,
        issuer: [u8; 32],
        nullifier: [u8; 32],
        verification_id: [u8; 31],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            &ctx.remaining_accounts[system_accounts_offset as usize..],
            crate::LIGHT_CPI_SIGNER,
        );
        let address_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let (address, address_seed) = derive_address(
            &[
                ZK_ID_CHECK,
                nullifier.as_slice(),
                verification_id.as_slice(),
            ],
            &address_pubkey,
            &crate::ID,
        );

        // Get root from input Merkle tree (example of reading on-chain state)
        let expected_root = read_state_merkle_tree_root(
            &ctx.accounts.input_merkle_tree.to_account_info(),
            input_root_index,
        )?;

        let merkle_tree_pubkey = ctx.accounts.input_merkle_tree.key();
        let merkle_tree_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&merkle_tree_pubkey.to_bytes()])
                .unwrap();

        let mut discriminator = [0u8; 32];
        discriminator[24..].copy_from_slice(CredentialAccount::LIGHT_DISCRIMINATOR_SLICE);
        let issuer_hashed = hashv_to_bn254_field_size_be_const_array::<2>(&[&issuer]).unwrap();
        let account_owner_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&crate::ID.to_bytes()]).unwrap();

        let mut event_account = LightAccount::<EncryptedEventAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );
        event_account.data = public_data;

        // Compute the data hash for the event account to use in ZK proof verification
        // Use SHA256 with length prefix to match the flat hashing scheme
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(&(event_account.data.len() as u32).to_le_bytes());
        hash_input.extend_from_slice(&event_account.data);
        let mut event_data_hash =
            Sha256::hash(&hash_input).map_err(|_| ProgramError::InvalidAccountData)?;
        event_data_hash[0] = 0; // Ensure hash is in BN254 field

        {
            // Construct public inputs array for the circuit
            // Order MUST match the circuit's public declaration exactly:
            // owner_hashed, merkle_tree_hashed, discriminator, issuer_hashed, expectedRoot, public_encrypted_data_hash, public_data_hash
            let mut padded_verification_id = [0u8; 32];
            padded_verification_id[1..].copy_from_slice(&verification_id);

            let public_inputs: [[u8; 32]; 8] = [
                account_owner_hashed,
                merkle_tree_hashed,
                discriminator,
                issuer_hashed,
                expected_root,
                padded_verification_id,
                event_data_hash, // This is public_encrypted_data_hash
                nullifier,
            ];
            msg!("public_inputs {:?}", public_inputs);

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
        }
        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(event_account)?
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
}
#[derive(Accounts)]
pub struct VerifyAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    /// CHECK: read_state_merkle_tree_root checks account owner, and discriminator
    pub input_merkle_tree: UncheckedAccount<'info>,
}

#[derive(
    Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct CredentialAccount {
    #[hash]
    pub issuer: Pubkey,
    /// CredentialPubkey (is a Poseidon hash -> no need to annotate with #[hash])
    pub credential_pubkey: CredentialPubkey,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct CredentialPubkey {
    pub credential_pubkey: Pubkey,
}

impl CredentialPubkey {
    pub fn new(credential_pubkey: Pubkey) -> Self {
        Self { credential_pubkey }
    }
}

// ToByteArray is required by LightHasher and not implemented for Pubkey or [u8;32],
// so we implement it here for CredentialPubkey.
impl ToByteArray for CredentialPubkey {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.credential_pubkey.to_bytes())
    }
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

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid issuer: signer is not the issuer of this account")]
    InvalidIssuer,
    #[msg("Not enough keys in remaining accounts")]
    AccountNotEnoughKeys,
}
