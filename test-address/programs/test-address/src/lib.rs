#![allow(unexpected_cfgs)]

use anchor_lang::{prelude::*, AnchorDeserialize, Discriminator};
// use light_compressed_account::address::derive_address;
use light_sdk::{
    address::v1::derive_address,
    cpi::{CpiAccounts, CpiInputs, CpiSigner},
    derive_light_cpi_signer,
    instruction::{PackedAddressTreeInfo, ValidityProof},
};

declare_id!("TestAddressProgram1111111111111111111111111");

#[constant]
pub const SUBMISSION_SEED: &[u8] = b"submission";

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("TestAddressProgram1111111111111111111111111");

#[program]
pub mod test_address {
    use super::*;

    pub fn claim<'info>(
        ctx: Context<'_, '_, '_, 'info, Claim<'info>>,
        submission_id: [u8; 32],
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
    ) -> Result<()> {
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.fee_payer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let tree = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .unwrap();
        msg!("onchain address tree: {:?}", tree.to_bytes());

        let (_address, address_seed) = derive_address(
            &[SUBMISSION_SEED, submission_id.as_ref()],
            &tree,
            &crate::ID,
        );

        let new_address_params = address_tree_info.into_new_address_params_packed(address_seed);

        let cpi_inputs = CpiInputs::new_with_address(proof, vec![], vec![new_address_params]);

        cpi_inputs
            .invoke_light_system_program(light_cpi_accounts)
            .map_err(ProgramError::from)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub fee_payer: Signer<'info>,
}

#[error_code]
pub enum TestAddressError {
    #[msg("Invalid submission")]
    InvalidSubmission,
}
