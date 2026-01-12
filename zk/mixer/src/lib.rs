#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_hasher::to_byte_array::ToByteArray;
use light_hasher::HasherError;
use light_sdk::account::poseidon::LightAccount as LightAccountPoseidon;
use light_sdk::account::LightAccount;
use light_sdk::cpi::v1::{CpiAccounts, LightSystemProgramCpi};
use light_sdk::cpi::{InvokeLightSystemProgram, LightCpiInstruction};
use light_sdk::instruction::{CompressedProof, PackedAddressTreeInfo, ValidityProof};
use light_sdk::merkle_tree::v1::read_state_merkle_tree_root;
use light_sdk::{
    address::v2::derive_address, derive_light_cpi_signer, LightDiscriminator, LightHasher,
};
use light_sdk_types::CpiSigner;

declare_id!("DUoztZJ377crfhkgS6a76MPkWF55ft7m4FGLLtJG3ZUx");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("DUoztZJ377crfhkgS6a76MPkWF55ft7m4FGLLtJG3ZUx");

pub const COMMITMENT: &[u8] = b"commitment";
pub const NULLIFIER: &[u8] = b"nullifier";

pub mod verifying_key;

#[program]
pub mod mixer {
    use groth16_solana::decompression::{decompress_g1, decompress_g2};
    use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;

    use super::*;

    /// Initializes a new mixer pool with a fixed denomination
    pub fn initialize(ctx: Context<Initialize>, denomination: u64) -> Result<()> {
        let config = &mut ctx.accounts.mixer_config;
        config.denomination = denomination;
        config.authority = ctx.accounts.authority.key();
        Ok(())
    }

    /// Deposits SOL and creates a commitment compressed account
    pub fn deposit<'info>(
        ctx: Context<'_, '_, '_, 'info, Deposit<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        commitment: [u8; 32],
    ) -> Result<()> {
        let config = &ctx.accounts.mixer_config;

        // Transfer SOL to vault
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.depositor.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                },
            ),
            config.denomination,
        )?;

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.depositor.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| MixerError::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            return Err(MixerError::InvalidAddressTree.into());
        }

        let (address, address_seed) = derive_address(
            &[COMMITMENT, commitment.as_slice()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut commitment_account = LightAccountPoseidon::<CommitmentAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );
        commitment_account.commitment = Commitment::new(commitment);

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account_poseidon(commitment_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Withdraws SOL by verifying a ZK proof and creating a nullifier account
    pub fn withdraw<'info>(
        ctx: Context<'_, '_, '_, 'info, Withdraw<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        input_root_index: u16,
        groth16_proof: CompressedProof,
        public_inputs: WithdrawPublicInputs,
    ) -> Result<()> {
        let config = &ctx.accounts.mixer_config;

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.payer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| MixerError::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            return Err(MixerError::InvalidAddressTree.into());
        }

        // Verify recipient matches public input
        require!(
            ctx.accounts.recipient.key().to_bytes() == public_inputs.recipient,
            MixerError::RecipientMismatch
        );

        // Read merkle tree root from the input merkle tree account
        let expected_root = read_state_merkle_tree_root(
            &ctx.accounts.input_merkle_tree.to_account_info(),
            input_root_index,
        )
        .map_err(|e| ProgramError::from(e))?;

        // Compute hashed values for circuit verification
        let merkle_tree_pubkey = ctx.accounts.input_merkle_tree.key();
        let merkle_tree_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&merkle_tree_pubkey.to_bytes()])
                .unwrap();

        let owner_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&crate::ID.to_bytes()]).unwrap();

        // Discriminator padded to 32 bytes (8-byte discriminator in the last 8 bytes)
        let mut discriminator = [0u8; 32];
        discriminator[24..].copy_from_slice(CommitmentAccount::LIGHT_DISCRIMINATOR_SLICE);

        // Verify Groth16 proof
        {
            // Hash recipient to fit in BN254 field (pubkeys are 256 bits, field is ~254 bits)
            let recipient_hashed =
                hashv_to_bn254_field_size_be_const_array::<2>(&[&public_inputs.recipient]).unwrap();

            // Public inputs order MUST match circuit declaration:
            // owner_hashed, merkle_tree_hashed, discriminator, expectedRoot, nullifierHash, recipient_hashed
            let circuit_public_inputs: [[u8; 32]; 6] = [
                owner_hashed,
                merkle_tree_hashed,
                discriminator,
                expected_root,
                public_inputs.nullifier_hash,
                recipient_hashed,
            ];

            let proof_a = decompress_g1(&groth16_proof.a).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;
            let proof_b = decompress_g2(&groth16_proof.b).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;
            let proof_c = decompress_g1(&groth16_proof.c).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;

            let mut verifier = Groth16Verifier::new(
                &proof_a,
                &proof_b,
                &proof_c,
                &circuit_public_inputs,
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

        // Create nullifier compressed account (prevents double-spend)
        let (nullifier_address, nullifier_seed) = derive_address(
            &[NULLIFIER, public_inputs.nullifier_hash.as_slice()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut nullifier_account = LightAccount::<NullifierAccount>::new_init(
            &crate::ID,
            Some(nullifier_address),
            output_state_tree_index,
        );
        nullifier_account.nullifier_hash = public_inputs.nullifier_hash;

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(nullifier_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(nullifier_seed)])
            .invoke(light_cpi_accounts)?;

        // Transfer SOL to recipient
        let config_key = ctx.accounts.mixer_config.key();
        let seeds: &[&[&[u8]]] = &[&[b"vault", config_key.as_ref(), &[ctx.bumps.vault]]];

        transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.recipient.to_account_info(),
                },
                seeds,
            ),
            config.denomination,
        )?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = MixerConfig::LEN)]
    pub mixer_config: Account<'info, MixerConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    pub mixer_config: Account<'info, MixerConfig>,
    /// CHECK: PDA vault holding deposited SOL
    #[account(mut, seeds = [b"vault", mixer_config.key().as_ref()], bump)]
    pub vault: AccountInfo<'info>,
    #[account(mut)]
    pub depositor: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub mixer_config: Account<'info, MixerConfig>,
    /// CHECK: PDA vault holding deposited SOL
    #[account(mut, seeds = [b"vault", mixer_config.key().as_ref()], bump)]
    pub vault: AccountInfo<'info>,
    /// CHECK: Validated against public inputs
    #[account(mut)]
    pub recipient: AccountInfo<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: read_state_merkle_tree_root checks account owner and discriminator
    pub input_merkle_tree: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct MixerConfig {
    pub authority: Pubkey,
    pub denomination: u64,
}

impl MixerConfig {
    pub const LEN: usize = 8 + 32 + 8;
}

#[derive(
    Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct CommitmentAccount {
    /// The commitment is a Poseidon hash of (nullifier, secret)
    /// This wrapper type makes the commitment the direct data_hash
    pub commitment: Commitment,
}

/// Wrapper type for commitment that implements ToByteArray
/// This allows the commitment to be used directly as the data_hash
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Commitment {
    pub value: [u8; 32],
}

impl Commitment {
    pub fn new(value: [u8; 32]) -> Self {
        Self { value }
    }
}

impl ToByteArray for Commitment {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.value)
    }
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct NullifierAccount {
    pub nullifier_hash: [u8; 32],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct WithdrawPublicInputs {
    pub nullifier_hash: [u8; 32],
    pub recipient: [u8; 32],
}

#[error_code]
pub enum MixerError {
    #[msg("Invalid merkle root")]
    InvalidRoot,
    #[msg("Recipient mismatch")]
    RecipientMismatch,
    #[msg("Invalid ZK proof")]
    InvalidProof,
    #[msg("Invalid address tree")]
    InvalidAddressTree,
    #[msg("Not enough keys in remaining accounts")]
    AccountNotEnoughKeys,
}
