use borsh::{BorshDeserialize, BorshSerialize};
use light_token::ValidityProof;
use light_sdk::instruction::PackedStateTreeInfo;
use solana_program::pubkey::Pubkey;

#[cfg(not(target_os = "solana"))]
use solana_program::instruction::{AccountMeta, Instruction};

#[cfg(not(target_os = "solana"))]
use light_compressed_token_sdk::compressed_token::batch_compress::{
    create_batch_compress_instruction, BatchCompressInputs, Recipient,
};
#[cfg(not(target_os = "solana"))]
use light_token::spl_interface::derive_spl_interface_pda;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct ClaimIxData {
    pub proof: ValidityProof,
    pub packed_tree_info: PackedStateTreeInfo,
    pub amount: u64,
    pub lamports: Option<u64>,
    pub mint: Pubkey,
    pub unlock_slot: u64,
    pub bump_seed: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub enum ClaimProgramInstruction {
    Claim(ClaimIxData),
}

#[cfg(not(target_os = "solana"))]
#[derive(Debug)]
pub struct ClaimAccounts {
    pub claimant: Pubkey,
    pub fee_payer: Pubkey,
    pub associated_airdrop_pda: Pubkey,
    pub ctoken_cpi_authority_pda: Pubkey,
    pub light_system_program: Pubkey,
    pub registered_program_pda: Pubkey,
    pub noop_program: Pubkey,
    pub account_compression_authority: Pubkey,
    pub account_compression_program: Pubkey,
    pub ctoken_program: Pubkey,
    pub token_pool_pda: Pubkey,
    pub decompress_destination: Pubkey,
    pub token_program: Pubkey,
    pub system_program: Pubkey,
    pub state_tree: Pubkey,
    pub queue: Pubkey,
}

/// Build a claim instruction in the client.
///
/// Accounts expected by this instruction:
///
///   0. `[signer]` Claimant
///   1. `[signer]` Fee payer
///   2. `[]` Associated airdrop PDA
///   3. `[]` CToken CPI authority PDA
///   4. `[]` Light system program
///   5. `[]` Registered program PDA
///   6. `[]` Noop program
///   7. `[]` Account compression authority
///   8. `[]` Account compression program
///   9. `[]` CToken program
///  10. `[]` Token pool PDA
///  11. `[writable]` Decompress destination
///  12. `[]` Token program
///  13. `[]` System program
///  14. `[writable]` State tree
///  15. `[writable]` Queue
#[cfg(not(target_os = "solana"))]
#[allow(clippy::too_many_arguments)]
pub fn build_claim_and_decompress_instruction(
    accounts: &ClaimAccounts,
    proof: ValidityProof,
    packed_tree_info: PackedStateTreeInfo,
    amount: u64,
    lamports: Option<u64>,
    mint: Pubkey,
    unlock_slot: u64,
    bump_seed: u8,
) -> Instruction {
    let accounts = vec![
        AccountMeta::new(accounts.claimant, true),
        AccountMeta::new(accounts.fee_payer, true),
        AccountMeta::new_readonly(accounts.associated_airdrop_pda, false),
        AccountMeta::new_readonly(accounts.ctoken_cpi_authority_pda, false),
        AccountMeta::new_readonly(accounts.light_system_program, false),
        AccountMeta::new_readonly(accounts.registered_program_pda, false),
        AccountMeta::new_readonly(accounts.noop_program, false),
        AccountMeta::new_readonly(accounts.account_compression_authority, false),
        AccountMeta::new_readonly(accounts.account_compression_program, false),
        AccountMeta::new_readonly(accounts.ctoken_program, false),
        AccountMeta::new(accounts.token_pool_pda, false),
        AccountMeta::new(accounts.decompress_destination, false),
        AccountMeta::new_readonly(accounts.token_program, false),
        AccountMeta::new_readonly(accounts.system_program, false),
        AccountMeta::new(accounts.state_tree, false),
        AccountMeta::new(accounts.queue, false),
    ];

    let instruction_data = ClaimProgramInstruction::Claim(ClaimIxData {
        proof,
        packed_tree_info,
        amount,
        lamports,
        mint,
        unlock_slot,
        bump_seed,
    });

    Instruction {
        program_id: crate::id(),
        accounts,
        data: borsh::to_vec(&instruction_data).unwrap(),
    }
}

/// Creates a compress instruction that compresses SPL tokens to a compressed token account.
///
/// # Arguments
/// * `fee_payer` - Account paying for the transaction
/// * `authority` - Owner of the sender token account (must be signer)
/// * `sender_token_account` - SPL token account to compress tokens from
/// * `mint` - Token mint address
/// * `amount` - Amount of tokens to compress
/// * `recipient` - Recipient of the compressed tokens (can be a PDA)
/// * `merkle_tree` - State tree to store the compressed account
#[cfg(not(target_os = "solana"))]
pub fn compress(
    fee_payer: Pubkey,
    authority: Pubkey,
    sender_token_account: Pubkey,
    mint: Pubkey,
    amount: u64,
    recipient: Pubkey,
    merkle_tree: Pubkey,
    token_program_id: Pubkey,
) -> Result<Instruction, light_token::error::TokenSdkError> {
    let spl_interface_info = derive_spl_interface_pda(&mint, 0, false);

    let inputs = BatchCompressInputs {
        fee_payer,
        authority,
        spl_interface_pda: spl_interface_info.pubkey,
        sender_token_account,
        token_program: token_program_id,
        merkle_tree,
        recipients: vec![Recipient {
            pubkey: recipient,
            amount,
        }],
        lamports: None,
        token_pool_index: spl_interface_info.index,
        token_pool_bump: spl_interface_info.bump,
        sol_pool_pda: None,
    };

    create_batch_compress_instruction(inputs)
}
