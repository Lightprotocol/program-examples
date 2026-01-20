#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use light_sdk::{
    account::LightAccount,
    address::v2::derive_address,
    cpi::{v2::CpiAccounts, CpiSigner},
    derive_light_cpi_signer,
    instruction::{PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator,
};
use light_sdk::constants::ADDRESS_TREE_V2;

declare_id!("CkmVPuPjBr6hcviQvoRKuHh9xk4pRmy9udm6egBk56wS");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("CkmVPuPjBr6hcviQvoRKuHh9xk4pRmy9udm6egBk56wS");

#[program]
pub mod create_nullifier {

    use super::*;
    use light_sdk::cpi::{
        v2::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction,
    };

    /// Creates a new rent-free account derived from id. If the id has been used
    /// before, the account exists, and the instruction will fail.
    pub fn create_account<'info>(
        ctx: Context<'_, '_, '_, 'info, GenericAnchorAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        id: [u8; 32],
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.signer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        let (address, address_seed) =
            derive_address(&[b"nullifier", &id], &address_tree_pubkey, &crate::ID);

        let my_compressed_account = LightAccount::<NullifierAccount>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        msg!("Created nullifier account with id: {:?}", id);

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(my_compressed_account)?
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

#[event]
#[derive(Clone, Debug, Default, LightDiscriminator)]
pub struct NullifierAccount {}
