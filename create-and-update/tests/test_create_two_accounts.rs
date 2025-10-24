#![cfg(feature = "test-sbf")]

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use create_and_update::{ByteDataAccount, DataAccount, FIRST_SEED, SECOND_SEED};
use light_client::indexer::TreeInfo;
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{PackedAccounts, SystemAccountMetaConfig},
};
use serial_test::serial;
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};

#[serial]
#[tokio::test]
async fn test_create_two_accounts() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("create_and_update", create_and_update::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    let (first_address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let (second_address, _) = derive_address(
        &[SECOND_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let byte_data = [1u8; 31]; // 31 bytes of data
    let message = "String account message".to_string();

    // Create two compressed accounts in a single instruction
    create_two_accounts(
        &mut rpc,
        &payer,
        &first_address,
        &second_address,
        address_tree_info,
        byte_data,
        message.clone(),
    )
    .await
    .unwrap();

    // Check that the first account (ByteDataAccount) was created correctly
    let first_compressed_account = rpc
        .get_compressed_account(first_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    let first_data = &first_compressed_account.data.as_ref().unwrap().data;
    let first_account_data = ByteDataAccount::deserialize(&mut &first_data[..]).unwrap();
    assert_eq!(first_account_data.owner, payer.pubkey());
    assert_eq!(first_account_data.data, byte_data);

    // Check that the second account (DataAccount) was created correctly
    let second_compressed_account = rpc
        .get_compressed_account(second_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    let second_data = &second_compressed_account.data.as_ref().unwrap().data;
    let second_account_data = DataAccount::deserialize(&mut &second_data[..]).unwrap();
    assert_eq!(second_account_data.owner, payer.pubkey());
    assert_eq!(second_account_data.message, message);
}

#[allow(clippy::too_many_arguments)]
async fn create_two_accounts<R>(
    rpc: &mut R,
    payer: &Keypair,
    first_address: &[u8; 32],
    second_address: &[u8; 32],
    address_tree_info: TreeInfo,
    byte_data: [u8; 31],
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(create_and_update::ID);
    remaining_accounts.add_system_accounts(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![
                AddressWithTree {
                    address: *first_address,
                    tree: address_tree_info.tree,
                },
                AddressWithTree {
                    address: *second_address,
                    tree: address_tree_info.tree,
                },
            ],
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

    let instruction_data = create_and_update::instruction::CreateTwoAccounts {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        byte_data,
        message,
    };

    let accounts = create_and_update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [accounts.to_account_metas(None), {
            let (metas, _, _) = remaining_accounts.to_account_metas();
            metas
        }]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
