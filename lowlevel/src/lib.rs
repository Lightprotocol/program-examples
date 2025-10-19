#![allow(unexpected_cfgs)]

use account_compression::{state_merkle_tree_from_bytes_zero_copy, StateMerkleTreeAccount};
use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use light_compressed_account::instruction_data::with_account_info::{
    CompressedAccountInfo, OutAccountInfo,
};
use light_sdk::cpi::v1::CpiAccounts;
use light_sdk::{
    address::v1::derive_address,
    cpi::{v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction},
    derive_light_cpi_signer,
    instruction::{PackedAddressTreeInfo, ValidityProof},
};
use light_sdk_types::CpiSigner;

declare_id!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("HNqStLMpNuNJqhBF1FbGTKHEFbBLJmq8RdJJmZKWz6jH");

pub const FIRST_SEED: &[u8] = b"first";
pub const SECOND_SEED: &[u8] = b"second";

#[program]
pub mod lowlevel {

    use super::*;

    /// Creates a new compressed account with initial data
    pub fn create_address_and_output_without_address<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof, // Required for the address create, it proves that the address does not exist yet in the light address tree.
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        input_root_index: u16,
        encrypted_utxo: Vec<u8>,    // must be checked by your zkp
        output_utxo_hash: [u8; 32], // must be checked by your zkp
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

        let (_address, address_seed) = derive_address(
            &[FIRST_SEED, ctx.accounts.signer.key().as_ref()],
            &address_pubkey,
            &crate::ID,
        );
        // Get root from input Merkle tree (example of reading on-chain state)
        let _input_root = read_merkle_tree_root(&ctx.accounts.input_merkle_tree, input_root_index)?;

        // Create output compressed account using low-level CompressedAccountInfo
        let output_account = CompressedAccountInfo {
            address: None,
            input: None,
            output: Some(OutAccountInfo {
                discriminator: 1u64.to_le_bytes(),
                output_merkle_tree_index: output_state_tree_index,
                lamports: 0,
                data: encrypted_utxo,
                data_hash: output_utxo_hash,
            }),
        };

        // Use new CPI builder pattern:
        // - Creates 1 output compressed account without address
        // - Creates 1 address without compressed account
        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_account_infos(&[output_account])
            .with_new_addresses(&[
                address_tree_info.into_new_address_params_assigned_packed(address_seed, None)
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
