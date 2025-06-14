//! Compressed Counter Program Tests
//!
//! This module contains integration tests for a compressed counter program built on the Light Protocol.
//! The tests demonstrate how to interact with compressed accounts on Solana, which are stored
//! off-chain in Merkle trees but verified on-chain for reduced storage costs.
//!
//! Key concepts demonstrated:
//! - Compressed accounts: Account data stored in Merkle trees instead of on-chain
//! - Validity proofs: Zero-knowledge proofs that verify compressed account state
//! - Address derivation: Deterministic generation of compressed account addresses
//! - State transitions: How compressed accounts are updated through nullification and creation

#![cfg(feature = "test-sbf")]

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use counter::CounterAccount;
use light_client::indexer::{CompressedAccount, TreeInfo};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v1::derive_address,
    instruction::{
        account_meta::{CompressedAccountMeta, CompressedAccountMetaClose},
        PackedAccounts, SystemAccountMetaConfig,
    },
};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};

/// Integration test for the compressed counter program.
///
/// This test demonstrates the full lifecycle of a compressed account:
/// 1. Create - Initialize a new compressed counter account
/// 2. Increment - Modify the counter value upward
/// 3. Decrement - Modify the counter value downward
/// 4. Reset - Set the counter back to zero
/// 5. Close - Permanently delete the compressed account
///
/// Each operation requires generating validity proofs to verify the current state
/// and creating new compressed accounts with updated data.
#[tokio::test]
async fn test_counter() {
    // Initialize the Light Protocol test environment with our counter program
    let config = ProgramTestConfig::new(true, Some(vec![("counter", counter::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // Get the address tree info needed for deriving compressed account addresses
    let address_tree_info = rpc.get_address_tree_v1();

    // Derive a deterministic address for our counter using the payer's pubkey as a seed
    // This creates a Program Derived Address (PDA) specific to this user's counter
    let (address, _) = derive_address(
        &[b"counter", payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &counter::ID,
    );

    // Create the counter compressed account
    create_counter(&mut rpc, &payer, &address, address_tree_info)
        .await
        .unwrap();

    // Verify the counter was created correctly at leaf index 0 with value 0
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    assert_eq!(compressed_account.leaf_index, 0);
    let counter = &compressed_account.data.as_ref().unwrap().data;
    let counter = CounterAccount::deserialize(&mut &counter[..]).unwrap();
    assert_eq!(counter.value, 0);

    // Increment the counter (nullifies old account, creates new one with value 1)
    increment_counter(&mut rpc, &payer, &compressed_account)
        .await
        .unwrap();

    // Verify the counter was incremented and is now at leaf index 1
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    assert_eq!(compressed_account.leaf_index, 1);
    let counter = &compressed_account.data.as_ref().unwrap().data;
    let counter = CounterAccount::deserialize(&mut &counter[..]).unwrap();
    assert_eq!(counter.value, 1);

    // Decrement the counter back to 0
    decrement_counter(&mut rpc, &payer, &compressed_account)
        .await
        .unwrap();

    // Verify the counter was decremented and is now at leaf index 2
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    assert_eq!(compressed_account.leaf_index, 2);

    let counter = &compressed_account.data.as_ref().unwrap().data;
    let counter = CounterAccount::deserialize(&mut &counter[..]).unwrap();
    assert_eq!(counter.value, 0);

    // Reset the counter (should maintain value 0 but create new account)
    reset_counter(&mut rpc, &payer, &compressed_account)
        .await
        .unwrap();

    // Verify the counter was reset correctly
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    let counter = &compressed_account.data.as_ref().unwrap().data;
    let counter = CounterAccount::deserialize(&mut &counter[..]).unwrap();
    assert_eq!(counter.value, 0);

    // Close the counter (permanently delete the compressed account)
    close_counter(&mut rpc, &payer, &compressed_account)
        .await
        .unwrap();

    // Verify no compressed accounts exist for this program after closing
    let compressed_accounts = rpc
        .get_compressed_accounts_by_owner(&counter::ID, None, None)
        .await
        .unwrap();
    assert_eq!(compressed_accounts.value.items.len(), 0);
}

/// Creates a new compressed counter account.
///
/// This function demonstrates the process of creating a compressed account:
/// 1. Set up system accounts required for Light Protocol operations
/// 2. Generate a validity proof for the new address (proving it doesn't exist)
/// 3. Pack the tree information and account metadata
/// 4. Build and send the instruction to create the compressed account
///
/// # Arguments
/// * `rpc` - The RPC client for interacting with the Light Protocol
/// * `payer` - The keypair that will pay for the transaction
/// * `address` - The derived address for the new compressed account
/// * `address_tree_info` - Information about the address tree where the account will be stored
async fn create_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Initialize the account metadata container for system accounts
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(counter::ID);
    remaining_accounts.add_system_accounts(config);

    // Generate a validity proof for creating a new account at this address
    // Empty vec for nullifier hashes (no accounts being nullified)
    // AddressWithTree specifies where the new account will be created
    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                tree: address_tree_info.tree,
                address: *address,
            }],
            None,
        )
        .await?
        .value;

    // Select an output state tree and pack its index into remaining accounts
    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    // Pack the address tree information into the remaining accounts
    let packed_address_tree_info = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees[0];

    // Build the instruction data with the proof and tree information
    let instruction_data = counter::instruction::CreateCounter {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index,
    };

    // Set up the primary accounts (just the signer in this case)
    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    // Convert packed accounts to AccountMeta format for the instruction
    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    // Build the complete instruction
    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    // Send the transaction
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

/// Increments the counter value in a compressed account.
///
/// This demonstrates the compressed account update pattern:
/// 1. Generate a validity proof for the current account (proving its existence and state)
/// 2. Nullify the old account and create a new one with incremented value
/// 3. Pack all required tree information and account metadata
///
/// # Arguments
/// * `rpc` - The RPC client for interacting with the Light Protocol
/// * `payer` - The keypair that will pay for the transaction
/// * `compressed_account` - The current compressed account to increment
#[allow(clippy::too_many_arguments)]
async fn increment_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Set up system accounts required for the operation
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(counter::ID);
    remaining_accounts.add_system_accounts(config);

    // Get the hash of the account we want to nullify (the current counter state)
    let hash = compressed_account.hash;

    // Generate validity proof for nullifying this account
    // The hash goes in the first parameter (accounts to nullify)
    // Empty vec for new addresses (we're updating, not creating new addresses)
    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    // Pack the state tree information where the nullified and new accounts will be stored
    let packed_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .state_trees
        .unwrap();

    // Deserialize the current counter data to read its value
    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    // Create metadata for the compressed account being updated
    let account_meta = CompressedAccountMeta {
        tree_info: packed_tree_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_tree_accounts.output_tree_index,
    };

    // Build instruction data with the proof, current value, and account metadata
    let instruction_data = counter::instruction::IncrementCounter {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta,
    };

    // Set up the primary accounts
    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    // Convert packed accounts to AccountMeta format
    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    // Build and send the instruction
    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

/// Decrements the counter value in a compressed account.
///
/// Similar to increment_counter, this nullifies the existing account and creates
/// a new one with the decremented value. The process is identical except for
/// the instruction type and the resulting counter value.
///
/// # Arguments
/// * `rpc` - The RPC client for interacting with the Light Protocol
/// * `payer` - The keypair that will pay for the transaction
/// * `compressed_account` - The current compressed account to decrement
#[allow(clippy::too_many_arguments)]
async fn decrement_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Set up system accounts
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(counter::ID);
    remaining_accounts.add_system_accounts(config);

    // Get the hash of the current account to nullify
    let hash = compressed_account.hash;

    // Generate validity proof for the nullification
    let rpc_result = rpc
        .get_validity_proof(Vec::from(&[hash]), vec![], None)
        .await?
        .value;

    // Pack state tree information
    let packed_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .state_trees
        .unwrap();

    // Deserialize current counter data
    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    // Create compressed account metadata
    let account_meta = CompressedAccountMeta {
        tree_info: packed_tree_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_tree_accounts.output_tree_index,
    };

    // Build decrement instruction
    let instruction_data = counter::instruction::DecrementCounter {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta,
    };

    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

/// Resets the counter value to zero.
///
/// This operation nullifies the current compressed account and creates a new one
/// with the counter value set to zero, regardless of the previous value.
///
/// # Arguments
/// * `rpc` - The RPC client for interacting with the Light Protocol
/// * `payer` - The keypair that will pay for the transaction
/// * `compressed_account` - The current compressed account to reset
async fn reset_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Set up system accounts
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(counter::ID);
    remaining_accounts.add_system_accounts(config);

    // Get the hash of the current account
    let hash = compressed_account.hash;

    // Generate validity proof
    let rpc_result = rpc
        .get_validity_proof(Vec::from(&[hash]), vec![], None)
        .await?
        .value;

    // Pack Merkle tree context information
    let packed_merkle_context = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .state_trees
        .unwrap();

    // Deserialize current counter data
    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    // Create compressed account metadata
    let account_meta = CompressedAccountMeta {
        tree_info: packed_merkle_context.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_merkle_context.output_tree_index,
    };

    // Build reset instruction
    let instruction_data = counter::instruction::ResetCounter {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta,
    };

    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

/// Closes (permanently deletes) the compressed counter account.
///
/// This operation nullifies the compressed account without creating a new one,
/// effectively removing it from the system. Note the use of CompressedAccountMetaClose
/// instead of CompressedAccountMeta since no output account is created.
///
/// # Arguments
/// * `rpc` - The RPC client for interacting with the Light Protocol
/// * `payer` - The keypair that will pay for the transaction
/// * `compressed_account` - The compressed account to close
async fn close_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Set up system accounts
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(counter::ID);
    remaining_accounts.add_system_accounts(config);

    // Get the hash of the account to close
    let hash = compressed_account.hash;

    // Generate validity proof for nullification
    let rpc_result = rpc
        .get_validity_proof(Vec::from(&[hash]), vec![], None)
        .await
        .unwrap()
        .value;

    // Pack tree information
    let packed_tree_infos = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .state_trees
        .unwrap();

    // Deserialize current counter data
    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    // Use CompressedAccountMetaClose since we're not creating an output account
    let account_meta = CompressedAccountMetaClose {
        tree_info: packed_tree_infos.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
    };

    // Build close instruction
    let instruction_data = counter::instruction::CloseCounter {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta,
    };

    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
