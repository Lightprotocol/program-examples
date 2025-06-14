// #![cfg(feature = "test-sbf")]

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use create_and_update::{
    DataAccount, ExistingCompressedAccountIxData, NewCompressedAccountIxData, FIRST_SEED,
    SECOND_SEED,
};
use light_client::indexer::{CompressedAccount, TreeInfo};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v1::derive_address,
    instruction::{account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig},
};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};

/// Tests basic compressed account creation functionality
#[tokio::test]
async fn test_create_compressed_account() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("create_and_update", create_and_update::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();

    // Derive deterministic address using seed and owner pubkey
    let (address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    // Create the compressed account
    create_compressed_account(
        &mut rpc,
        &payer,
        &address,
        address_tree_info,
        "Hello, World!".to_string(),
    )
    .await
    .unwrap();

    // Verify account creation and data integrity
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    assert_eq!(compressed_account.leaf_index, 0);
    let data = &compressed_account.data.as_ref().unwrap().data;
    let account_data = DataAccount::deserialize(&mut &data[..]).unwrap();
    assert_eq!(account_data.owner, payer.pubkey());
    assert_eq!(account_data.message, "Hello, World!");
}

/// Tests composite operations: creating a new account while updating an existing one,
/// followed by updating both accounts simultaneously
#[tokio::test]
async fn test_create_and_update() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("create_and_update", create_and_update::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();

    let (initial_address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    // Create the initial compressed account
    create_compressed_account(
        &mut rpc,
        &payer,
        &initial_address,
        address_tree_info,
        "Initial message".to_string(),
    )
    .await
    .unwrap();

    // Get the created account for updating
    let initial_compressed_account = rpc
        .get_compressed_account(initial_address, None)
        .await
        .unwrap()
        .value;

    // Execute atomic create-and-update operation
    create_and_update_accounts(
        &mut rpc,
        &payer,
        &initial_compressed_account,
        "Initial message".to_string(),
        "New account message".to_string(),
        "Updated message".to_string(),
    )
    .await
    .unwrap();

    // Verify new account creation with SECOND_SEED
    let (new_address, _) = derive_address(
        &[SECOND_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let new_compressed_account = rpc
        .get_compressed_account(new_address, None)
        .await
        .unwrap()
        .value;

    let new_data = &new_compressed_account.data.as_ref().unwrap().data;
    let new_account_data = DataAccount::deserialize(&mut &new_data[..]).unwrap();
    assert_eq!(new_account_data.owner, payer.pubkey());
    assert_eq!(new_account_data.message, "New account message");

    // Verify existing account was updated
    let updated_compressed_account = rpc
        .get_compressed_account(initial_address, None)
        .await
        .unwrap()
        .value;

    let updated_data = &updated_compressed_account.data.as_ref().unwrap().data;
    let updated_account_data = DataAccount::deserialize(&mut &updated_data[..]).unwrap();
    assert_eq!(updated_account_data.owner, payer.pubkey());
    assert_eq!(updated_account_data.message, "Updated message");

    // Test batch update of both existing accounts
    update_two_accounts(
        &mut rpc,
        &payer,
        &updated_compressed_account,
        "Updated message".to_string(),
        "First account final message".to_string(),
        &new_compressed_account,
        "New account message".to_string(),
        "Second account final message".to_string(),
    )
    .await
    .unwrap();

    // Verify both accounts were updated correctly
    let final_first_account = rpc
        .get_compressed_account(initial_address, None)
        .await
        .unwrap()
        .value;

    let final_first_data = &final_first_account.data.as_ref().unwrap().data;
    let final_first_account_data = DataAccount::deserialize(&mut &final_first_data[..]).unwrap();
    assert_eq!(
        final_first_account_data.message,
        "First account final message"
    );

    let final_second_account = rpc
        .get_compressed_account(new_address, None)
        .await
        .unwrap()
        .value;

    let final_second_data = &final_second_account.data.as_ref().unwrap().data;
    let final_second_account_data = DataAccount::deserialize(&mut &final_second_data[..]).unwrap();
    assert_eq!(
        final_second_account_data.message,
        "Second account final message"
    );
}

/// Creates a new compressed account at the specified address with the given message
async fn create_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(create_and_update::ID);
    remaining_accounts.add_system_accounts(config);

    // Get validity proof for address creation (no existing accounts to prove)
    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: *address,
                tree: address_tree_info.tree,
            }],
            None,
        )
        .await?
        .value;

    // Pack tree accounts for CPI and get output state tree for new account
    let packed_address_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees;
    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let instruction_data = create_and_update::instruction::CreateCompressedAccount {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        message,
    };

    let accounts = create_and_update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

/// Atomically creates a new compressed account and updates an existing one in a single transaction
async fn create_and_update_accounts<R>(
    rpc: &mut R,
    payer: &Keypair,
    existing_account: &CompressedAccount,
    existing_message: String,
    new_message: String,
    update_message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(create_and_update::ID);
    remaining_accounts.add_system_accounts(config);

    let hash = existing_account.hash;

    let address_tree_info = rpc.get_address_tree_v1();

    // Derive address for the new account using SECOND_SEED
    let (new_address, _) = derive_address(
        &[SECOND_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let address_tree_info = rpc.get_address_tree_v1();

    // Get validity proof: existing account hash to prove it exists, new address to prove it doesn't
    let rpc_result = rpc
        .get_validity_proof(
            vec![hash],
            vec![AddressWithTree {
                address: new_address,
                tree: address_tree_info.tree,
            }],
            None,
        )
        .await?
        .value;
    println!("rpc_result {:?}", rpc_result);
    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();
    let packed_address_tree_accounts = packed_tree_accounts.address_trees;

    // Create metadata for the existing account being updated
    let account_meta = CompressedAccountMeta {
        tree_info: packed_state_tree_accounts.packed_tree_infos[0],
        address: existing_account.address.unwrap(),
        output_state_tree_index: packed_state_tree_accounts.output_tree_index,
    };

    let instruction_data = create_and_update::instruction::CreateAndUpdate {
        proof: rpc_result.proof,
        existing_account: ExistingCompressedAccountIxData {
            account_meta,
            message: existing_message,
            update_message,
        },
        new_account: NewCompressedAccountIxData {
            address_tree_info: packed_address_tree_accounts[0],
            message: new_message,
        },
    };

    let accounts = create_and_update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

/// Updates two existing compressed accounts in a single atomic transaction
#[allow(clippy::too_many_arguments)]
async fn update_two_accounts<R>(
    rpc: &mut R,
    payer: &Keypair,
    first_account: &CompressedAccount,
    first_current_message: String,
    first_update_message: String,
    second_account: &CompressedAccount,
    second_current_message: String,
    second_update_message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(create_and_update::ID);
    remaining_accounts.add_system_accounts(config);

    let first_hash = first_account.hash;
    let second_hash = second_account.hash;

    // Get validity proof for both existing accounts (no new addresses needed)
    let rpc_result = rpc
        .get_validity_proof(vec![first_hash, second_hash], vec![], None)
        .await?
        .value;

    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();

    // Create metadata for both accounts being updated
    let first_account_meta = CompressedAccountMeta {
        tree_info: packed_state_tree_accounts.packed_tree_infos[0],
        address: first_account.address.unwrap(),
        output_state_tree_index: packed_state_tree_accounts.output_tree_index,
    };

    let second_account_meta = CompressedAccountMeta {
        tree_info: packed_state_tree_accounts.packed_tree_infos[1],
        address: second_account.address.unwrap(),
        output_state_tree_index: packed_state_tree_accounts.output_tree_index,
    };

    let instruction_data = create_and_update::instruction::UpdateTwoAccounts {
        proof: rpc_result.proof,
        first_account: ExistingCompressedAccountIxData {
            account_meta: first_account_meta,
            message: first_current_message,
            update_message: first_update_message,
        },
        second_account: ExistingCompressedAccountIxData {
            account_meta: second_account_meta,
            message: second_current_message,
            update_message: second_update_message,
        },
    };

    let accounts = create_and_update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
