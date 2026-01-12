#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;
use light_sdk::account::LightAccount;
use light_sdk::cpi::v1::{CpiAccounts, LightSystemProgramCpi};
use light_sdk::cpi::{InvokeLightSystemProgram, LightCpiInstruction};
use light_sdk::instruction::{CompressedProof, PackedAddressTreeInfo, ValidityProof};
use light_sdk::{address::v2::derive_address, derive_light_cpi_signer, LightDiscriminator};
use light_sdk_types::CpiSigner;

declare_id!("AnonymousAirdrop111111111111111111111111111");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("AnonymousAirdrop111111111111111111111111111");

pub const NULLIFIER_SEED: &[u8] = b"nullifier";

pub mod error;
pub mod verifying_key;

use error::AirdropError;

#[program]
pub mod zk_airdrop_claim {
    use groth16_solana::decompression::{decompress_g1, decompress_g2};

    use super::*;

    /// Initializes a new airdrop with eligibility Merkle root and configuration
    pub fn initialize_airdrop(
        ctx: Context<InitializeAirdrop>,
        airdrop_id: u64,
        eligibility_root: [u8; 32],
        unlock_slot: u64,
        amount_per_claim: u64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.airdrop_config;
        config.airdrop_id = airdrop_id;
        config.authority = ctx.accounts.authority.key();
        config.eligibility_root = eligibility_root;
        config.unlock_slot = unlock_slot;
        config.amount_per_claim = amount_per_claim;
        config.is_active = true;
        config.bump = ctx.bumps.airdrop_config;
        Ok(())
    }

    /// Claims SOL anonymously by providing a ZK proof
    ///
    /// Privacy guarantee: Observer sees "Someone claimed X lamports to address Y"
    /// but cannot tell which eligible address from the snapshot is claiming.
    pub fn claim<'info>(
        ctx: Context<'_, '_, '_, 'info, Claim<'info>>,
        // Light Protocol validity proof for address creation
        validity_proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        // Groth16 proof from circuit
        groth16_proof: CompressedProof,
        // Public inputs
        nullifier: [u8; 32],
    ) -> Result<()> {
        let config = &ctx.accounts.airdrop_config;

        // Check airdrop is active
        require!(config.is_active, AirdropError::AirdropNotActive);

        // Check time-lock
        let clock = Clock::get()?;
        require!(clock.slot >= config.unlock_slot, AirdropError::TokensLocked);

        let amount = config.amount_per_claim;
        let recipient_key = ctx.accounts.recipient.key();

        // Set up Light Protocol CPI
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.payer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| AirdropError::AccountNotEnoughKeys)?;

        // Verify address tree is valid
        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            return Err(AirdropError::InvalidAddressTree.into());
        }

        // Hash recipient to BN254 field for circuit verification
        let recipient_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&recipient_key.to_bytes()]).unwrap();

        // Hash airdrop_id to BN254 field
        let airdrop_id_bytes = config.airdrop_id.to_le_bytes();
        let airdrop_id_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&airdrop_id_bytes]).unwrap();

        // Amount as 32-byte big-endian
        let mut amount_bytes = [0u8; 32];
        amount_bytes[24..].copy_from_slice(&amount.to_be_bytes());

        // Verify Groth16 proof
        {
            // Public inputs order MUST match circuit declaration:
            // eligibilityRoot, nullifier, recipient, airdropId, amount
            let circuit_public_inputs: [[u8; 32]; 5] = [
                config.eligibility_root,
                nullifier,
                recipient_hashed,
                airdrop_id_hashed,
                amount_bytes,
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

            verifier.verify().map_err(|_| AirdropError::InvalidProof)?;
        }

        // Create nullifier account (prevents double-claim)
        // If address already exists, Light Protocol CPI will fail
        let (nullifier_address, nullifier_seed) = derive_address(
            &[NULLIFIER_SEED, nullifier.as_slice()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut nullifier_account = LightAccount::<NullifierAccount>::new_init(
            &crate::ID,
            Some(nullifier_address),
            output_state_tree_index,
        );
        nullifier_account.nullifier = nullifier;

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, validity_proof)
            .with_light_account(nullifier_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(nullifier_seed)])
            .invoke(light_cpi_accounts)?;

        // Transfer SOL from vault to recipient
        let airdrop_id_bytes = config.airdrop_id.to_le_bytes();
        let seeds: &[&[&[u8]]] = &[&[b"airdrop", airdrop_id_bytes.as_ref(), &[config.bump]]];

        transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.recipient.to_account_info(),
                },
                seeds,
            ),
            amount,
        )?;

        Ok(())
    }

    /// Deactivates the airdrop (authority only)
    pub fn deactivate_airdrop(ctx: Context<DeactivateAirdrop>) -> Result<()> {
        ctx.accounts.airdrop_config.is_active = false;
        Ok(())
    }
}

// ============ ACCOUNTS ============

#[derive(Accounts)]
#[instruction(airdrop_id: u64)]
pub struct InitializeAirdrop<'info> {
    #[account(
        init,
        payer = authority,
        space = AirdropConfig::LEN,
        seeds = [b"airdrop", airdrop_id.to_le_bytes().as_ref()],
        bump
    )]
    pub airdrop_config: Account<'info, AirdropConfig>,

    /// CHECK: PDA vault for holding airdrop funds
    #[account(
        mut,
        seeds = [b"vault", airdrop_id.to_le_bytes().as_ref()],
        bump
    )]
    pub vault: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    pub airdrop_config: Account<'info, AirdropConfig>,

    /// CHECK: PDA vault holding airdrop funds
    #[account(
        mut,
        seeds = [b"vault", airdrop_config.airdrop_id.to_le_bytes().as_ref()],
        bump
    )]
    pub vault: AccountInfo<'info>,

    /// CHECK: Recipient can be any address
    #[account(mut)]
    pub recipient: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DeactivateAirdrop<'info> {
    #[account(
        mut,
        has_one = authority
    )]
    pub airdrop_config: Account<'info, AirdropConfig>,

    pub authority: Signer<'info>,
}

// ============ STATE ============

/// Configuration for an anonymous airdrop
#[account]
pub struct AirdropConfig {
    /// Unique identifier for this airdrop
    pub airdrop_id: u64,
    /// Authority who can deactivate the airdrop
    pub authority: Pubkey,
    /// Merkle root of (eligible_address, amount) pairs
    pub eligibility_root: [u8; 32],
    /// Slot when funds become claimable
    pub unlock_slot: u64,
    /// Amount per claim in lamports
    pub amount_per_claim: u64,
    /// Whether the airdrop is active
    pub is_active: bool,
    /// PDA bump seed
    pub bump: u8,
}

impl AirdropConfig {
    pub const LEN: usize = 8 + // discriminator
        8 +  // airdrop_id
        32 + // authority
        32 + // eligibility_root
        8 +  // unlock_slot
        8 +  // amount_per_claim
        1 +  // is_active
        1; // bump
}

/// Compressed account that prevents double-claims
/// Address derived from: [b"nullifier", nullifier_hash]
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct NullifierAccount {
    /// The nullifier = Poseidon(airdrop_id, private_key)
    pub nullifier: [u8; 32],
}
