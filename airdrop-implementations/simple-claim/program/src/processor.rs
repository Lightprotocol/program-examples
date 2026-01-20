use crate::{
    error::ClaimError,
    instruction::{ClaimIxData, ClaimProgramInstruction},
};
use borsh::BorshDeserialize;

use light_token::compressed_token::{
    transfer::instruction::DecompressInputs, CTokenAccount, TokenAccountMeta,
};
use solana_program::{
    account_info::AccountInfo, clock::Clock, entrypoint::ProgramResult, msg,
    program::invoke_signed, program_error::ProgramError, pubkey::Pubkey, sysvar::Sysvar,
};

pub fn process_instruction(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
    let instruction = ClaimProgramInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    match instruction {
        ClaimProgramInstruction::Claim(ix_data) => process_claim(accounts, ix_data),
    }
}

#[allow(clippy::too_many_arguments)]
fn process_claim(accounts: &[AccountInfo], ix_data: ClaimIxData) -> ProgramResult {
    let ClaimIxData {
        proof,
        packed_tree_info,
        amount,
        lamports,
        mint,
        unlock_slot,
        bump_seed,
    } = ix_data;
    let claimant_info = &accounts[0];
    let fee_payer_info = &accounts[1];
    let associated_airdrop_pda_info = &accounts[2];
    let _ctoken_cpi_authority_pda_info = &accounts[3];
    let _light_system_program_info = &accounts[4];
    let _registered_program_pda_info = &accounts[5];
    let _noop_program_info = &accounts[6];
    let _account_compression_authority_info = &accounts[7];
    let _account_compression_program_info = &accounts[8];
    let ctoken_program_info = &accounts[9];
    let spl_interface_pda_info = &accounts[10];
    let decompress_destination_info = &accounts[11];
    let token_program_info = &accounts[12];
    let _system_program_info = &accounts[13];
    let state_tree_info = &accounts[14];
    let queue_info = &accounts[15];

    if accounts.len() != 16 {
        msg!("Expected 16 accounts, got {}", accounts.len());
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // CHECK:
    if !claimant_info.is_signer {
        msg!("Claimant must be a signer");
        claimant_info.key.log();
        return Err(ProgramError::MissingRequiredSignature);
    }
    // CHECK:
    if !fee_payer_info.is_signer {
        msg!("Fee payer must be a signer");
        fee_payer_info.key.log();
        return Err(ProgramError::MissingRequiredSignature);
    }
    // CHECK:
    if ctoken_program_info.key != &light_token::instruction::id() {
        msg!("Invalid compressed token program.",);
        ctoken_program_info.key.log();
        return Err(ProgramError::InvalidArgument);
    }
    // CHECK:
    if token_program_info.key != &spl_token::ID {
        msg!("Invalid SPL token program.");
        token_program_info.key.log();
        return Err(ProgramError::InvalidArgument);
    }
    let compressed_token_account = CTokenAccount::new(
        mint,
        associated_airdrop_pda_info.key.clone(),
        vec![TokenAccountMeta {
            amount,
            delegate_index: None,
            packed_tree_info,
            lamports,
            tlv: None,
        }],
        1,
    );
    let decompress_inputs = DecompressInputs {
        fee_payer: fee_payer_info.key.clone(),
        validity_proof: proof,
        sender_account: compressed_token_account,
        amount,
        tree_pubkeys: vec![state_tree_info.key.clone(), queue_info.key.clone()],
        config: None,
        spl_interface_pda: spl_interface_pda_info.key.clone(),
        recipient_token_account: decompress_destination_info.key.clone(),
        spl_token_program: token_program_info.key.clone(),
    };

    let instruction =
        light_token::compressed_token::transfer::instruction::decompress(decompress_inputs)?;

    // CHECK:
    let current_slot = Clock::get()?.slot;
    if current_slot < unlock_slot {
        msg!(
            "Tokens are still locked: current slot ({}) is less than unlock slot ({}).",
            current_slot,
            unlock_slot
        );
        return Err(ClaimError::TokensLocked.into());
    }

    let claimant_bytes = claimant_info.key.to_bytes();
    let slot_bytes = unlock_slot.to_le_bytes();
    let mint_bytes = mint.to_bytes();

    let seeds = &[
        &claimant_bytes[..32],
        &mint_bytes[..32],
        &slot_bytes[..8],
        &[bump_seed],
    ];

    check_claim_pda(seeds, &crate::ID, associated_airdrop_pda_info.key)?;

    let signers_seeds: &[&[&[u8]]] = &[&seeds[..]];
    invoke_signed(&instruction, &accounts, signers_seeds)?;
    Ok(())
}

fn check_claim_pda(
    seeds: &[&[u8]],
    claim_program: &Pubkey,
    airdrop_account: &Pubkey,
) -> Result<(), ProgramError> {
    let derived_pda =
        Pubkey::create_program_address(seeds, claim_program).expect("Invalid PDA seeds.");

    if derived_pda != *airdrop_account {
        msg!(
            "Invalid airdrop PDA provided. Expected: {}. Found: {}.",
            derived_pda,
            airdrop_account
        );
        return Err(ClaimError::InvalidPDA.into());
    }

    Ok(())
}
