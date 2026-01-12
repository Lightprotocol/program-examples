#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_hasher::to_byte_array::ToByteArray;
use light_hasher::HasherError;
use light_sdk::account::poseidon::LightAccount as LightAccountPoseidon;
use light_sdk::account::LightAccount;
use light_sdk::cpi::v1::CpiAccounts;
use light_sdk::{
    address::v2::derive_address,
    cpi::{v1::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
    derive_light_cpi_signer,
    instruction::{CompressedProof, PackedAddressTreeInfo, ValidityProof},
    merkle_tree::v1::read_state_merkle_tree_root,
    LightDiscriminator, LightHasher,
};
use light_sdk_types::CpiSigner;

declare_id!("Dgskb2KpCssabWhgMTwttqbJ9UF98edoiWoukzz1wX2H");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("Dgskb2KpCssabWhgMTwttqbJ9UF98edoiWoukzz1wX2H");

pub const BALANCE: &[u8] = b"balance";
pub const NULLIFIER: &[u8] = b"nullifier";
pub const VAULT: &[u8] = b"vault";

pub mod verifying_key;

#[program]
pub mod confidential_transfer {
    use groth16_solana::decompression::{decompress_g1, decompress_g2};
    use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;

    use super::*;

    /// Initialize a vault for a specific token mint
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let vault_config = &mut ctx.accounts.vault_config;
        vault_config.mint = ctx.accounts.mint.key();
        vault_config.authority = ctx.accounts.authority.key();
        vault_config.total_deposits = 0;
        Ok(())
    }

    /// Deposit tokens and create a private balance commitment
    /// commitment = Poseidon(amount, blinding)
    pub fn deposit<'info>(
        ctx: Context<'_, '_, '_, 'info, Deposit<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        amount: u64,
        _blinding: [u8; 32],
        commitment: [u8; 32],
    ) -> Result<()> {
        require!(amount > 0, PaymentsError::ZeroAmount);

        // Transfer tokens to vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.depositor_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.depositor.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update vault config
        let vault_config = &mut ctx.accounts.vault_config;
        vault_config.total_deposits = vault_config
            .total_deposits
            .checked_add(amount)
            .ok_or(PaymentsError::Overflow)?;

        // Create balance commitment compressed account
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.depositor.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| PaymentsError::AccountNotEnoughKeys)?;

        let (address, address_seed) = derive_address(
            &[BALANCE, commitment.as_slice()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mint_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[ctx.accounts.mint.key().as_ref()])
                .unwrap();

        let mut balance_account = LightAccountPoseidon::<BalanceCommitment>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );
        balance_account.mint_hashed = MintHashed::new(mint_hashed);
        balance_account.commitment = Commitment::new(commitment);

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account_poseidon(balance_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Create a balance commitment directly (for testing without token transfer)
    pub fn create_balance_commitment<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateBalanceCommitmentAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        commitment: [u8; 32],
        mint_hashed: [u8; 32],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| PaymentsError::AccountNotEnoughKeys)?;

        let (address, address_seed) = derive_address(
            &[BALANCE, commitment.as_slice()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut balance_account = LightAccountPoseidon::<BalanceCommitment>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );
        balance_account.mint_hashed = MintHashed::new(mint_hashed);
        balance_account.commitment = Commitment::new(commitment);

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account_poseidon(balance_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Transfer tokens privately using ZK proof
    /// Creates nullifier + receiver commitment (sender change handled separately)
    pub fn transfer<'info>(
        ctx: Context<'_, '_, '_, 'info, TransferPrivate<'info>>,
        proof: ValidityProof,
        nullifier_address_tree_info: PackedAddressTreeInfo,
        receiver_address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        input_root_index: u16,
        transfer_proof: CompressedProof,
        nullifier: [u8; 32],
        receiver_commitment: [u8; 32],
        mint_hashed: [u8; 32],
    ) -> Result<()> {
        // Verify Groth16 proof in separate scope to minimize stack
        {
            let expected_root = read_state_merkle_tree_root(
                &ctx.accounts.input_merkle_tree.to_account_info(),
                input_root_index,
            )
            .map_err(ProgramError::from)?;

            let merkle_tree_hashed = hashv_to_bn254_field_size_be_const_array::<2>(&[ctx
                .accounts
                .input_merkle_tree
                .key()
                .as_ref()])
            .unwrap();

            let owner_hashed =
                hashv_to_bn254_field_size_be_const_array::<2>(&[crate::ID.as_ref()]).unwrap();

            let mut discriminator = [0u8; 32];
            discriminator[24..].copy_from_slice(BalanceCommitment::LIGHT_DISCRIMINATOR_SLICE);

            let public_inputs: [[u8; 32]; 7] = [
                owner_hashed,
                merkle_tree_hashed,
                discriminator,
                mint_hashed,
                expected_root,
                nullifier,
                receiver_commitment,
            ];

            let proof_a = decompress_g1(&transfer_proof.a).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;
            let proof_b = decompress_g2(&transfer_proof.b).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;
            let proof_c = decompress_g1(&transfer_proof.c).map_err(|e| {
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
        }

        // Create accounts after proof verification
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let nullifier_tree_pubkey = nullifier_address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| PaymentsError::AccountNotEnoughKeys)?;

        let receiver_tree_pubkey = receiver_address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| PaymentsError::AccountNotEnoughKeys)?;

        // Create nullifier account (prevents double-spend)
        let (nullifier_address, nullifier_seed) = derive_address(
            &[NULLIFIER, nullifier.as_slice()],
            &nullifier_tree_pubkey,
            &crate::ID,
        );

        let mut nullifier_account = LightAccount::<NullifierAccount>::new_init(
            &crate::ID,
            Some(nullifier_address),
            output_state_tree_index,
        );
        nullifier_account.nullifier = nullifier;

        // Create receiver balance commitment
        let (receiver_address, receiver_seed) = derive_address(
            &[BALANCE, receiver_commitment.as_slice()],
            &receiver_tree_pubkey,
            &crate::ID,
        );

        let mut receiver_account = LightAccount::<BalanceCommitment>::new_init(
            &crate::ID,
            Some(receiver_address),
            output_state_tree_index,
        );
        receiver_account.mint_hashed = MintHashed::new(mint_hashed);
        receiver_account.commitment = Commitment::new(receiver_commitment);

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(nullifier_account)?
            .with_light_account(receiver_account)?
            .with_new_addresses(&[
                nullifier_address_tree_info.into_new_address_params_packed(nullifier_seed),
                receiver_address_tree_info.into_new_address_params_packed(receiver_seed),
            ])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Withdraw tokens by proving ownership of a commitment
    pub fn withdraw<'info>(
        ctx: Context<'_, '_, '_, 'info, Withdraw<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        _input_root_index: u16,
        amount: u64,
        nullifier: [u8; 32],
    ) -> Result<()> {
        require!(amount > 0, PaymentsError::ZeroAmount);

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.withdrawer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| PaymentsError::AccountNotEnoughKeys)?;

        // Create nullifier account
        let (nullifier_address, nullifier_seed) = derive_address(
            &[NULLIFIER, nullifier.as_slice()],
            &address_tree_pubkey,
            &crate::ID,
        );

        let mut nullifier_account = LightAccount::<NullifierAccount>::new_init(
            &crate::ID,
            Some(nullifier_address),
            output_state_tree_index,
        );
        nullifier_account.nullifier = nullifier;

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(nullifier_account)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(nullifier_seed)])
            .invoke(light_cpi_accounts)?;

        // Transfer tokens from vault to withdrawer
        let vault_config_key = ctx.accounts.vault_config.key();
        let seeds: &[&[&[u8]]] = &[&[
            VAULT,
            vault_config_key.as_ref(),
            &[ctx.bumps.vault_token_account],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.withdrawer_token_account.to_account_info(),
            authority: ctx.accounts.vault_token_account.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            seeds,
        );
        token::transfer(cpi_ctx, amount)?;

        // Update vault config
        let vault_config = &mut ctx.accounts.vault_config;
        vault_config.total_deposits = vault_config
            .total_deposits
            .checked_sub(amount)
            .ok_or(PaymentsError::InsufficientFunds)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = VaultConfig::LEN,
        seeds = [b"config", mint.key().as_ref()],
        bump
    )]
    pub vault_config: Account<'info, VaultConfig>,
    #[account(
        init,
        payer = authority,
        token::mint = mint,
        token::authority = vault_token_account,
        seeds = [VAULT, vault_config.key().as_ref()],
        bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    pub mint: Account<'info, token::Mint>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut, seeds = [b"config", mint.key().as_ref()], bump)]
    pub vault_config: Account<'info, VaultConfig>,
    #[account(mut, seeds = [VAULT, vault_config.key().as_ref()], bump)]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub depositor_token_account: Account<'info, TokenAccount>,
    pub mint: Account<'info, token::Mint>,
    #[account(mut)]
    pub depositor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CreateBalanceCommitmentAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct TransferPrivate<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    /// CHECK: Validated by read_state_merkle_tree_root
    pub input_merkle_tree: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut, seeds = [b"config", mint.key().as_ref()], bump)]
    pub vault_config: Account<'info, VaultConfig>,
    #[account(mut, seeds = [VAULT, vault_config.key().as_ref()], bump)]
    pub vault_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub withdrawer_token_account: Account<'info, TokenAccount>,
    pub mint: Account<'info, token::Mint>,
    #[account(mut)]
    pub withdrawer: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct VaultConfig {
    pub mint: Pubkey,
    pub authority: Pubkey,
    pub total_deposits: u64,
}

impl VaultConfig {
    pub const LEN: usize = 8 + 32 + 32 + 8;
}

#[derive(
    Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher,
)]
pub struct BalanceCommitment {
    pub mint_hashed: MintHashed,
    pub commitment: Commitment,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct MintHashed(pub [u8; 32]);

impl MintHashed {
    pub fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }
}

impl ToByteArray for MintHashed {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.0)
    }
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Commitment(pub [u8; 32]);

impl Commitment {
    pub fn new(commitment: [u8; 32]) -> Self {
        Self(commitment)
    }
}

impl ToByteArray for Commitment {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.0)
    }
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct NullifierAccount {
    pub nullifier: [u8; 32],
}

#[error_code]
pub enum PaymentsError {
    #[msg("Zero amount")]
    ZeroAmount,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient funds in vault")]
    InsufficientFunds,
    #[msg("Not enough keys in remaining accounts")]
    AccountNotEnoughKeys,
    #[msg("Invalid proof")]
    InvalidProof,
}
