#![cfg(feature = "test-sbf")]

use borsh::{BorshDeserialize, BorshSerialize};
use counter::{
    CloseCounterInstructionData, CounterAccount, CreateCounterInstructionData,
    DecrementCounterInstructionData, IncrementCounterInstructionData, ResetCounterInstructionData,
};
use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::address::v1::derive_address;
use light_sdk::instruction::{
    account_meta::{CompressedAccountMeta, CompressedAccountMetaClose},
    PackedAccounts, SystemAccountMetaConfig,
};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

/// Integration test for the counter program using Light Protocol's compressed accounts.
/// This test demonstrates the complete lifecycle of a counter: create, increment, decrement, reset, and close.
///
/// Light Protocol enables state compression on Solana, allowing for more efficient storage
/// by storing account data in Merkle trees rather than as individual accounts.
#[tokio::test]
async fn test_counter() {
    // Initialize the Light Protocol test environment with the counter program
    let config = ProgramTestConfig::new(true, Some(vec![("counter", counter::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // Get the address tree for compressed account address derivation
    let address_tree_info = rpc.get_address_tree_v1();
    let address_tree_pubkey = address_tree_info.tree;

    // Create counter - derive a deterministic address based on program ID and seed
    let (address, _) = derive_address(
        &[b"counter", payer.pubkey().as_ref()],
        &address_tree_pubkey,
        &counter::ID,
    );
    // Get a random Merkle tree to store the compressed account state
    let merkle_tree_pubkey = rpc.get_random_state_tree_info().unwrap().tree;

    create_counter(
        &payer,
        &mut rpc,
        &merkle_tree_pubkey,
        address_tree_pubkey,
        address,
    )
    .await
    .unwrap();

    // Verify the counter was created successfully by fetching it from the compressed state
    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    assert_eq!(compressed_counter.address.unwrap(), address);

    // Test increment operation
    increment_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    // Fetch updated state after increment
    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    // Test decrement operation
    decrement_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    // Fetch updated state after decrement
    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    // Test reset operation (sets counter back to 0)
    reset_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    // Fetch updated state after reset
    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    // Test close operation (removes the account and reclaims rent)
    close_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();
}

/// Creates a new counter compressed account.
///
/// This function demonstrates the pattern for creating compressed accounts:
/// 1. Set up account metadata configuration
/// 2. Generate validity proof for the new address
/// 3. Pack tree information and account metadata
/// 4. Build and send the instruction
///
/// # Arguments
/// * `payer` - The keypair that will pay for transaction fees and sign the transaction
/// * `rpc` - The Light Protocol test RPC client
/// * `merkle_tree_pubkey` - The Merkle tree where the compressed account state will be stored
/// * `address_tree_pubkey` - The address tree used for address derivation and validation
/// * `address` - The derived address for the new counter account
pub async fn create_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    merkle_tree_pubkey: &Pubkey,
    address_tree_pubkey: Pubkey,
    address: [u8; 32],
) -> Result<(), RpcError> {
    // Configure system accounts needed for compressed account operations
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config);

    // Generate validity proof for creating a new compressed account at the specified address
    // Empty input accounts (vec![]) since we're creating, not modifying existing accounts
    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address,
                tree: address_tree_pubkey,
            }],
            None,
        )
        .await?
        .value;

    // Pack the Merkle tree and address tree information into the account metadata
    let output_merkle_tree_index = accounts.insert_or_get(*merkle_tree_pubkey);
    let packed_address_tree_info = rpc_result.pack_tree_infos(&mut accounts).address_trees[0];
    let (accounts, _, _) = accounts.to_account_metas();

    // Build instruction data with proof and tree information
    let instruction_data = CreateCounterInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index: output_merkle_tree_index,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    // Create the instruction with program ID, accounts, and serialized data
    let instruction = Instruction {
        program_id: counter::ID,
        accounts,
        data: [
            &[counter::InstructionType::CreateCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    // Submit the transaction to create the counter
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

/// Increments the counter value by 1.
///
/// This demonstrates the pattern for modifying compressed accounts:
/// 1. Extract the current account hash for the validity proof
/// 2. Generate proof that the account exists and can be modified
/// 3. Deserialize current account data to get the current counter value
/// 4. Build instruction with proof and current state
///
/// # Arguments
/// * `payer` - The keypair that will pay for transaction fees and sign the transaction
/// * `rpc` - The Light Protocol test RPC client
/// * `compressed_account` - The current compressed account state to increment
pub async fn increment_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    // Set up system accounts for the compressed account operation
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config);

    // Get the hash of the current account state for the validity proof
    let hash = compressed_account.hash;

    // Generate validity proof that this account exists and can be consumed
    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    // Pack the state tree information
    let packed_accounts = rpc_result
        .pack_tree_infos(&mut accounts)
        .state_trees
        .unwrap();

    // Deserialize the current counter account to access its value
    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    // Build metadata for the compressed account being modified
    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (accounts, _, _) = accounts.to_account_metas();
    let instruction_data = IncrementCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts,
        data: [
            &[counter::InstructionType::IncrementCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

/// Decrements the counter value by 1.
///
/// This follows the same pattern as increment_counter but calls the decrement instruction.
/// The program logic will handle preventing underflow below zero.
///
/// # Arguments
/// * `payer` - The keypair that will pay for transaction fees and sign the transaction
/// * `rpc` - The Light Protocol test RPC client
/// * `compressed_account` - The current compressed account state to decrement
pub async fn decrement_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config);

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_accounts = rpc_result
        .pack_tree_infos(&mut accounts)
        .state_trees
        .unwrap();

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (accounts, _, _) = accounts.to_account_metas();
    let instruction_data = DecrementCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts,
        data: [
            &[counter::InstructionType::DecrementCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

/// Resets the counter value back to 0.
///
/// This operation modifies the compressed account state, similar to increment/decrement,
/// but sets the counter to a specific value (0) regardless of the current value.
///
/// # Arguments
/// * `payer` - The keypair that will pay for transaction fees and sign the transaction
/// * `rpc` - The Light Protocol test RPC client
/// * `compressed_account` - The current compressed account state to reset
pub async fn reset_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config);

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_accounts = rpc_result
        .pack_tree_infos(&mut accounts)
        .state_trees
        .unwrap();

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (accounts, _, _) = accounts.to_account_metas();
    let instruction_data = ResetCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts,
        data: [
            &[counter::InstructionType::ResetCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

/// Closes the counter account, removing it from the compressed state tree.
///
/// This operation demonstrates account closure in the compressed account model.
/// Unlike regular Solana accounts, compressed accounts use CompressedAccountMetaClose
/// which doesn't specify an output tree since the account is being removed, not updated.
///
/// # Arguments
/// * `payer` - The keypair that will pay for transaction fees and sign the transaction
/// * `rpc` - The Light Protocol test RPC client
/// * `compressed_account` - The compressed account to close
pub async fn close_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config);

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_accounts = rpc_result
        .pack_tree_infos(&mut accounts)
        .state_trees
        .unwrap();

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    // Use CompressedAccountMetaClose for account closure (no output_state_tree_index needed)
    let meta_close = CompressedAccountMetaClose {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
    };

    let (accounts, _, _) = accounts.to_account_metas();
    let instruction_data = CloseCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta_close,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts,
        data: [
            &[counter::InstructionType::CloseCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
