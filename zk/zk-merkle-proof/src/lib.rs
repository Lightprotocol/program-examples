#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_hasher::to_byte_array::ToByteArray;
use light_hasher::HasherError;
use light_sdk::account::poseidon::LightAccount as LightAccountPoseidon;
use light_sdk::address::v2::derive_address;
use light_sdk::cpi::v1::CpiAccounts;
use light_sdk::{
    cpi::{v1::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
    derive_light_cpi_signer,
    instruction::{CompressedProof, PackedAddressTreeInfo, ValidityProof},
    merkle_tree::v1::read_state_merkle_tree_root,
    LightDiscriminator, LightHasher,
};
use light_sdk_types::CpiSigner;

declare_id!("MPzkYomvQc4VQPwMr6bFduyWRQZVCh5CofgDC4dFqJp");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("MPzkYomvQc4VQPwMr6bFduyWRQZVCh5CofgDC4dFqJp");

pub const ZK_ACCOUNT: &[u8] = b"zk_account";

pub mod verifying_key;

#[program]
pub mod zk_merkle_proof {
    use groth16_solana::decompression::{decompress_g1, decompress_g2};
    use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;

    use super::*;

    pub fn create_account<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateAccountAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        data_hash: [u8; 32],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ProgramError::InvalidAccountData)?;

        let (address, address_seed) = derive_address(
            &[ZK_ACCOUNT, &data_hash],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut account = LightAccountPoseidon::<ZkAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        account.data_hash = DataHash(data_hash);

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account_poseidon(account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    pub fn verify_account<'info>(
        ctx: Context<'_, '_, '_, 'info, VerifyAccountAccounts<'info>>,
        input_root_index: u16,
        zk_proof: CompressedProof,
        data_hash: [u8; 32],
    ) -> Result<()> {
        let expected_root = read_state_merkle_tree_root(
            &ctx.accounts.state_merkle_tree.to_account_info(),
            input_root_index,
        )
        .map_err(ProgramError::from)?;

        let owner_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&crate::ID.to_bytes()]).unwrap();

        let merkle_tree_pubkey = ctx.accounts.state_merkle_tree.key();
        let merkle_tree_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&merkle_tree_pubkey.to_bytes()])
                .unwrap();

        let mut discriminator = [0u8; 32];
        discriminator[24..].copy_from_slice(ZkAccount::LIGHT_DISCRIMINATOR_SLICE);

        let public_inputs: [[u8; 32]; 5] = [
            owner_hashed,
            merkle_tree_hashed,
            discriminator,
            data_hash,
            expected_root,
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

        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateAccountAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct VerifyAccountAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    /// CHECK: validated by read_state_merkle_tree_root
    pub state_merkle_tree: UncheckedAccount<'info>,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher)]
pub struct ZkAccount {
    pub data_hash: DataHash,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct DataHash(pub [u8; 32]);

impl ToByteArray for DataHash {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.0)
    }
}
