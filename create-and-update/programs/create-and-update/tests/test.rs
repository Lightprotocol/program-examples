#![cfg(feature = "test-sbf")]

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
    address::v2::derive_address,
    instruction::{account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig},
};
use serial_test::serial;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};

#[serial]
#[tokio::test]
async fn test_create_compressed_account() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("create_and_update", create_and_update::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

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

    // Check that it was created correctly
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    assert_eq!(compressed_account.leaf_index, 0);
    let data = &compressed_account.data.as_ref().unwrap().data;
    let account_data = DataAccount::deserialize(&mut &data[..]).unwrap();
    assert_eq!(account_data.owner, payer.pubkey());
    assert_eq!(account_data.message, "Hello, World!");
}

#[serial]
#[tokio::test]
async fn test_create_and_update() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("create_and_update", create_and_update::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

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
        .value
        .unwrap();

    // Create and update in one instruction
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

    // Check the new account was created
    let (new_address, _) = derive_address(
        &[SECOND_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let new_compressed_account = rpc
        .get_compressed_account(new_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    let new_data = &new_compressed_account.data.as_ref().unwrap().data;
    let new_account_data = DataAccount::deserialize(&mut &new_data[..]).unwrap();
    assert_eq!(new_account_data.owner, payer.pubkey());
    assert_eq!(new_account_data.message, "New account message");

    // Check the existing account was updated
    let updated_compressed_account = rpc
        .get_compressed_account(initial_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    let updated_data = &updated_compressed_account.data.as_ref().unwrap().data;
    let updated_account_data = DataAccount::deserialize(&mut &updated_data[..]).unwrap();
    assert_eq!(updated_account_data.owner, payer.pubkey());
    assert_eq!(updated_account_data.message, "Updated message");

    // Now test updating both existing accounts with the third instruction
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

    // Check both accounts were updated correctly
    let final_first_account = rpc
        .get_compressed_account(initial_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

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
        .value
        .unwrap();

    let final_second_data = &final_second_account.data.as_ref().unwrap().data;
    let final_second_account_data = DataAccount::deserialize(&mut &final_second_data[..]).unwrap();
    assert_eq!(
        final_second_account_data.message,
        "Second account final message"
    );
}

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
    remaining_accounts.add_system_accounts_v2(config)?;

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

    let (remaining_metas, _, _) = remaining_accounts.to_account_metas();
    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [accounts.to_account_metas(None), remaining_metas].concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

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
    remaining_accounts.add_system_accounts_v2(config)?;

    let hash = existing_account.hash;

    let address_tree_info = rpc.get_address_tree_v2();

    let (new_address, _) = derive_address(
        &[SECOND_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let address_tree_info = rpc.get_address_tree_v2();

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

    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();
    let packed_address_tree_accounts = packed_tree_accounts.address_trees;
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

    let (remaining_metas, _, _) = remaining_accounts.to_account_metas();
    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [accounts.to_account_metas(None), remaining_metas].concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

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
    remaining_accounts.add_system_accounts_v2(config)?;

    let first_hash = first_account.hash;
    let second_hash = second_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![first_hash, second_hash], vec![], None)
        .await?
        .value;

    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();

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

    let (remaining_metas, _, _) = remaining_accounts.to_account_metas();
    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [accounts.to_account_metas(None), remaining_metas].concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
