//! Vendored copy of `light_sdk::merkle_tree::v1::read_state_merkle_tree_root`.
//!
//! The released `light-sdk` 0.23.0 exposed this helper behind its `merkle-tree`
//! feature, but the local SDK checkout this example builds against dropped the
//! module. The behaviour is reproduced here verbatim; it reads a historical root
//! from a concurrent state Merkle tree account by index, validating the account
//! owner and discriminator first.

use anchor_lang::prelude::*;
use light_concurrent_merkle_tree::zero_copy::ConcurrentMerkleTreeZeroCopy;
use light_hasher::Poseidon;

/// `StateMerkleTreeAccount` discriminator.
pub const STATE_MERKLE_TREE_DISCRIMINATOR: [u8; 8] = [172, 43, 172, 186, 29, 73, 219, 84];
pub const STATE_MERKLE_TREE_ACCOUNT_METADATA_LEN: usize = 224;

/// Reads a root from the concurrent state Merkle tree by index.
pub fn read_state_merkle_tree_root(
    account_info: &AccountInfo,
    root_index: u16,
) -> Result<[u8; 32]> {
    if root_index as usize >= 2400 {
        msg!(
            "Invalid root index: {} greater than max root index {}",
            root_index,
            2400
        );
        return Err(ProgramError::InvalidArgument.into());
    }

    // Validate the account is owned by the account-compression program.
    let account_compression_program_id =
        Pubkey::new_from_array(light_sdk::constants::ACCOUNT_COMPRESSION_PROGRAM_ID);
    if account_info.owner != &account_compression_program_id {
        msg!("StateMerkleTreeAccount owned by wrong program");
        return Err(ProgramError::IllegalOwner.into());
    }

    let account_data = account_info.try_borrow_data()?;

    // Check discriminator.
    if account_data.len() < 8 {
        msg!("StateMerkleTreeAccount data too short for discriminator");
        return Err(ProgramError::InvalidAccountData.into());
    }

    let discriminator = account_data
        .get(0..8)
        .ok_or(ProgramError::InvalidAccountData)?;
    if discriminator != STATE_MERKLE_TREE_DISCRIMINATOR {
        msg!("Invalid StateMerkleTreeAccount discriminator");
        return Err(ProgramError::InvalidAccountData.into());
    }

    let required_size = STATE_MERKLE_TREE_ACCOUNT_METADATA_LEN;
    if account_data.len() < required_size {
        msg!("StateMerkleTreeAccount data too short for metadata");
        return Err(ProgramError::InvalidAccountData.into());
    }

    let data = account_data
        .get(required_size..)
        .ok_or(ProgramError::InvalidAccountData)?;
    let merkle_tree = ConcurrentMerkleTreeZeroCopy::<Poseidon, 26>::from_bytes_zero_copy(data)
        .map_err(|_| ProgramError::InvalidAccountData)?;

    merkle_tree
        .roots
        .get(root_index as usize)
        .copied()
        .ok_or_else(|| ProgramError::InvalidArgument.into())
}
