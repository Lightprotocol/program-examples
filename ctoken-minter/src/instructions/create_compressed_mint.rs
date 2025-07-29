use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use light_compressed_token_sdk::account_infos::CreateCompressedMintAccountInfos;
use light_compressed_token_sdk::instructions::{
    create_compressed_mint_cpi, CreateCompressedMintInputs,
};
use light_compressed_token_sdk::CompressedProof;
use light_ctoken_types::instructions::extensions::{
    ExtensionInstructionData, TokenMetadataInstructionData,
};

#[derive(Accounts)]
pub struct CreateCompressedMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub mint_authority: Signer<'info>,
}

#[derive(Debug, Clone, AnchorDeserialize, AnchorSerialize)]
pub struct CreateCompressedMintInstructionData {
    pub decimals: u8,
    pub freeze_authority: Option<Pubkey>,
    pub proof: CompressedProof,
    pub mint_bump: u8,
    pub address_merkle_tree_root_index: u16,
    pub version: u8,
    pub metadata: Option<TokenMetadataInstructionData>,
    pub compressed_mint_address: [u8; 32],
}

pub fn create_compressed_mint<'info>(
    ctx: Context<'_, '_, '_, 'info, CreateCompressedMint<'info>>,
    input: CreateCompressedMintInstructionData,
) -> Result<()> {
    let mint_cpi_account_infos =
        CreateCompressedMintAccountInfos::new(ctx.accounts.payer.as_ref(), ctx.remaining_accounts);

    let create_mint_inputs = CreateCompressedMintInputs {
        mint_bump: input.mint_bump,
        address_merkle_tree_root_index: input.address_merkle_tree_root_index,
        version: input.version,
        decimals: input.decimals,
        extensions: input
            .metadata
            .map(|metadata| vec![ExtensionInstructionData::TokenMetadata(metadata)]),
        freeze_authority: input.freeze_authority,
        mint_authority: ctx.accounts.mint_authority.key(),
        proof: input.proof,
        mint_signer: *mint_cpi_account_infos.mint_signer().unwrap().key,
        payer: ctx.accounts.payer.key(),
        address_tree_pubkey: *mint_cpi_account_infos.address_merkle_tree().unwrap().key,
        output_queue: *mint_cpi_account_infos.out_output_queue().unwrap().key,
    };
    let create_mint_instruction =
        create_compressed_mint_cpi(create_mint_inputs, input.compressed_mint_address)
            .map_err(ProgramError::from)?;

    // Execute the CPI call to create the compressed mint
    invoke(
        &create_mint_instruction,
        mint_cpi_account_infos.to_account_infos().as_ref(),
    )?;

    Ok(())
}

#[error_code]
pub enum CreateCompressedMintErrorCode {
    #[msg("Token name cannot be empty")]
    InvalidTokenName,
    #[msg("Token symbol cannot be empty")]
    InvalidTokenSymbol,
    #[msg("Token URI cannot be empty")]
    InvalidTokenUri,
    #[msg("Decimals must be between 0 and 9")]
    InvalidDecimals,
    #[msg("Invalid proof provided")]
    InvalidProof,
}
