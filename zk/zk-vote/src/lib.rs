#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use borsh::{BorshDeserialize, BorshSerialize};
use groth16_solana::groth16::Groth16Verifier;
use light_hasher::to_byte_array::ToByteArray;
use light_hasher::HasherError;
use light_sdk::account::{poseidon::LightAccount as LightAccountPoseidon, LightAccount};
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

declare_id!("4en9qBTXDhZmMDBYNN9kzC84C5JGJ21GZfKQCEZVmMzh");

pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("4en9qBTXDhZmMDBYNN9kzC84C5JGJ21GZfKQCEZVmMzh");

// Account seeds
pub const POLL: &[u8] = b"poll";
pub const VOTER: &[u8] = b"voter";
pub const VOTE_RECORD: &[u8] = b"vote";

// Include the generated verifying key module
pub mod verifying_key;

#[program]
pub mod zk_vote {
    use groth16_solana::decompression::{decompress_g1, decompress_g2};
    use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;

    use super::*;

    /// Creates a new poll with a question and 3 voting options.
    /// The poll authority can register voters and close the poll.
    pub fn create_poll<'info>(
        ctx: Context<'_, '_, '_, 'info, CreatePollAccounts<'info>>,
        poll_id: u32,
        question: String,
        option_0: String,
        option_1: String,
        option_2: String,
    ) -> Result<()> {
        require!(question.len() <= 100, ErrorCode::QuestionTooLong);
        require!(option_0.len() <= 50, ErrorCode::OptionTooLong);
        require!(option_1.len() <= 50, ErrorCode::OptionTooLong);
        require!(option_2.len() <= 50, ErrorCode::OptionTooLong);

        let poll = &mut ctx.accounts.poll;
        poll.bump = ctx.bumps.poll;
        poll.id = poll_id;
        poll.authority = ctx.accounts.authority.key();
        poll.question = question;
        poll.options = [option_0, option_1, option_2];
        poll.vote_counts = [0, 0, 0];
        poll.is_open = true;

        msg!(
            "Created poll {} with question: {}",
            poll_id,
            poll.question
        );

        Ok(())
    }

    /// Registers a voter for a specific poll by creating a compressed VoterCredential.
    /// Only the poll authority can register voters.
    pub fn register_voter<'info>(
        ctx: Context<'_, '_, '_, 'info, RegisterVoterAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        credential_pubkey: [u8; 32],
    ) -> Result<()> {
        let poll = &ctx.accounts.poll;
        require!(poll.is_open, ErrorCode::PollClosed);

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.authority.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_tree_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_tree_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        // Derive address from poll_id and credential_pubkey
        let (address, address_seed) = derive_address(
            &[VOTER, &poll.id.to_le_bytes(), &credential_pubkey],
            &address_tree_pubkey,
            &crate::ID,
        );

        // Hash poll_id to BN254 field element for ZK circuit compatibility
        // The circuit expects: data_hash = Poseidon(poll_id_hashed, credential_pubkey)
        let poll_id_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&poll.id.to_le_bytes()]).unwrap();

        // Create compressed VoterCredential with Poseidon hashing for ZK compatibility
        let mut voter_credential = LightAccountPoseidon::<VoterCredential>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        voter_credential.poll_id_hashed = HashedPollId::new(poll_id_hashed);
        voter_credential.credential_pubkey = CredentialPubkey::new(credential_pubkey);

        msg!(
            "Registered voter credential for poll {}: {:?}",
            poll.id,
            credential_pubkey
        );

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account_poseidon(voter_credential)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Submits a vote with a ZK proof of credential ownership.
    /// The proof verifies the voter has a valid credential without revealing which one.
    /// Creates a VoteRecord at the nullifier-derived address to prevent double-voting.
    pub fn vote<'info>(
        ctx: Context<'_, '_, '_, 'info, VoteAccounts<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        input_root_index: u16,
        vote_choice: u8,
        credential_proof: CompressedProof,
        nullifier: [u8; 32],
    ) -> Result<()> {
        require!(vote_choice < 3, ErrorCode::InvalidVoteChoice);

        let poll = &mut ctx.accounts.poll;
        require!(poll.is_open, ErrorCode::PollClosed);

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.voter.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let address_pubkey = address_tree_info
            .get_tree_pubkey(&light_cpi_accounts)
            .map_err(|_| ErrorCode::AccountNotEnoughKeys)?;

        if address_pubkey.to_bytes() != light_sdk::constants::ADDRESS_TREE_V2 {
            msg!("Invalid address tree");
            return Err(ProgramError::InvalidAccountData.into());
        }

        // Derive VoteRecord address from nullifier and poll_id
        // This address will fail to create if a vote already exists (double-vote prevention)
        let (vote_record_address, address_seed) = derive_address(
            &[VOTE_RECORD, &nullifier, &poll.id.to_le_bytes()],
            &address_pubkey,
            &crate::ID,
        );

        // Get Merkle tree root for proof verification
        let expected_root = read_state_merkle_tree_root(
            &ctx.accounts.input_merkle_tree.to_account_info(),
            input_root_index,
        )
        .map_err(|e| ProgramError::from(e))?;

        // Hash public inputs for circuit verification
        let merkle_tree_pubkey = ctx.accounts.input_merkle_tree.key();
        let merkle_tree_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&merkle_tree_pubkey.to_bytes()])
                .unwrap();

        let mut discriminator = [0u8; 32];
        discriminator[24..].copy_from_slice(VoterCredential::LIGHT_DISCRIMINATOR_SLICE);

        let poll_id_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&poll.id.to_le_bytes()]).unwrap();

        let owner_hashed =
            hashv_to_bn254_field_size_be_const_array::<2>(&[&crate::ID.to_bytes()]).unwrap();

        // Construct public inputs array for the circuit
        // Order MUST match vote_proof.circom public declaration exactly
        let mut vote_choice_bytes = [0u8; 32];
        vote_choice_bytes[31] = vote_choice;

        let public_inputs: [[u8; 32]; 7] = [
            owner_hashed,
            merkle_tree_hashed,
            discriminator,
            poll_id_hashed,
            expected_root,
            nullifier,
            vote_choice_bytes,
        ];

        msg!("Verifying vote proof for poll {}", poll.id);

        // Verify the Groth16 ZK proof
        {
            let proof_a = decompress_g1(&credential_proof.a).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;

            let proof_b = decompress_g2(&credential_proof.b).map_err(|e| {
                let code: u32 = e.into();
                Error::from(ProgramError::Custom(code))
            })?;

            let proof_c = decompress_g1(&credential_proof.c).map_err(|e| {
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

        // Create VoteRecord to prevent double-voting
        let mut vote_record = LightAccount::<VoteRecord>::new_init(
            &crate::ID,
            Some(vote_record_address),
            output_state_tree_index,
        );
        vote_record.nullifier = nullifier;
        vote_record.poll_id = poll.id;
        vote_record.vote_choice = vote_choice;
        vote_record.timestamp = Clock::get()?.unix_timestamp;

        // Increment vote count
        poll.vote_counts[vote_choice as usize] = poll.vote_counts[vote_choice as usize]
            .checked_add(1)
            .ok_or(ProgramError::ArithmeticOverflow)?;

        msg!(
            "Vote recorded for poll {}: option {} (counts: {:?})",
            poll.id,
            vote_choice,
            poll.vote_counts
        );

        let vote_record_info = vote_record
            .to_output_compressed_account_with_packed_context(None)?
            .unwrap();

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_output_compressed_accounts(&[vote_record_info])
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        Ok(())
    }

    /// Closes the poll and emits the winner.
    /// Only the poll authority can close the poll.
    pub fn close_poll<'info>(ctx: Context<'_, '_, '_, 'info, ClosePollAccounts<'info>>) -> Result<()> {
        let poll = &mut ctx.accounts.poll;
        require!(poll.is_open, ErrorCode::PollClosed);

        poll.is_open = false;

        // Determine winner (index of max vote count)
        let mut winner = 0u8;
        let mut max_votes = poll.vote_counts[0];

        for (i, &count) in poll.vote_counts.iter().enumerate().skip(1) {
            if count > max_votes {
                max_votes = count;
                winner = i as u8;
            }
        }

        msg!(
            "Poll {} closed. Winner: {} ({}) with {} votes. Final counts: {:?}",
            poll.id,
            winner,
            poll.options[winner as usize],
            max_votes,
            poll.vote_counts
        );

        emit!(PollClosedEvent {
            poll_id: poll.id,
            winner,
            vote_counts: poll.vote_counts,
        });

        Ok(())
    }
}

// ============ ANCHOR ACCOUNTS ============

#[derive(Accounts)]
#[instruction(poll_id: u32)]
pub struct CreatePollAccounts<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + Poll::INIT_SPACE,
        seeds = [POLL, authority.key().as_ref(), &poll_id.to_le_bytes()],
        bump,
    )]
    pub poll: Account<'info, Poll>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterVoterAccounts<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        has_one = authority,
        constraint = poll.is_open @ ErrorCode::PollClosed
    )]
    pub poll: Account<'info, Poll>,
}

#[derive(Accounts)]
pub struct VoteAccounts<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,

    #[account(
        mut,
        constraint = poll.is_open @ ErrorCode::PollClosed
    )]
    pub poll: Account<'info, Poll>,

    /// CHECK: read_state_merkle_tree_root checks account owner and discriminator
    pub input_merkle_tree: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ClosePollAccounts<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        has_one = authority,
        constraint = poll.is_open @ ErrorCode::PollClosed
    )]
    pub poll: Account<'info, Poll>,
}

// ============ SOLANA PDA ACCOUNT ============

#[account]
#[derive(InitSpace)]
pub struct Poll {
    pub bump: u8,
    pub id: u32,
    pub authority: Pubkey,
    #[max_len(100)]
    pub question: String,
    #[max_len(50, 50, 50)]
    pub options: [String; 3],
    pub vote_counts: [u64; 3],
    pub is_open: bool,
}

// ============ COMPRESSED ACCOUNTS ============

/// Voter credential stored as compressed account with Poseidon hashing.
/// The credential_pubkey is a Poseidon commitment: Poseidon(private_key).
/// Used in ZK proofs to verify voting eligibility without revealing identity.
///
/// Note: Both fields are already hashed values (BN254 field elements), so they
/// are included directly in the data_hash without additional hashing.
/// data_hash = Poseidon(poll_id_hashed, credential_pubkey)
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator, LightHasher)]
pub struct VoterCredential {
    /// Poll ID hashed to BN254 field size (already a hash, no #[hash] attribute)
    pub poll_id_hashed: HashedPollId,
    /// Credential public key (Poseidon hash of private key, no #[hash] attribute)
    pub credential_pubkey: CredentialPubkey,
}

/// Wrapper for poll_id_hashed that implements ToByteArray for LightHasher.
/// Contains the BN254 field hash of the poll ID.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct HashedPollId(pub [u8; 32]);

impl HashedPollId {
    pub fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }
}

impl ToByteArray for HashedPollId {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.0)
    }
}

/// Wrapper for credential public key that implements ToByteArray for LightHasher.
/// Contains the Poseidon hash of the credential private key.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct CredentialPubkey(pub [u8; 32]);

impl CredentialPubkey {
    pub fn new(pubkey: [u8; 32]) -> Self {
        Self(pubkey)
    }
}

impl ToByteArray for CredentialPubkey {
    const NUM_FIELDS: usize = 1;
    fn to_byte_array(&self) -> std::result::Result<[u8; 32], HasherError> {
        Ok(self.0)
    }
}

/// Vote record stored as compressed account at nullifier-derived address.
/// The address derivation from nullifier ensures only one vote per credential per poll.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize, LightDiscriminator)]
pub struct VoteRecord {
    pub nullifier: [u8; 32],
    pub poll_id: u32,
    pub vote_choice: u8,
    pub timestamp: i64,
}

// ============ EVENTS ============

#[event]
pub struct PollClosedEvent {
    pub poll_id: u32,
    pub winner: u8,
    pub vote_counts: [u64; 3],
}

// ============ ERRORS ============

#[error_code]
pub enum ErrorCode {
    #[msg("Question must be 100 characters or less")]
    QuestionTooLong,
    #[msg("Option must be 50 characters or less")]
    OptionTooLong,
    #[msg("Poll is closed")]
    PollClosed,
    #[msg("Invalid vote choice: must be 0, 1, or 2")]
    InvalidVoteChoice,
    #[msg("Not enough keys in remaining accounts")]
    AccountNotEnoughKeys,
}
