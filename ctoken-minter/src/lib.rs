#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

pub mod instructions;
use instructions::*;

declare_id!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

#[program]
pub mod ctoken_minter {

    use super::*;

    /// Creates a compressed mint with token metadata
    pub fn create_compressed_mint<'info>(
        ctx: Context<'_, '_, '_, 'info, CreateCompressedMint<'info>>,
        inputs: CreateCompressedMintInstructionData,
    ) -> Result<()> {
        create_compressed_mint::create_compressed_mint(ctx, inputs)
    }

    /// Mints compressed tokens to recipients
    pub fn mint_compressed_tokens<'info>(
        ctx: Context<'_, '_, '_, 'info, MintCompressedTokens<'info>>,
        inputs: MintCompressedTokensInstructionData,
    ) -> Result<()> {
        mint_compressed_tokens::mint_compressed_tokens(ctx, inputs)
    }
}
