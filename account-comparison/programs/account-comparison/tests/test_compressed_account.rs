use account_comparison::CompressedAccountData;
use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
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

#[tokio::test]
async fn test_create_compressed_account() {
    let name = "Heinrich".to_string();

    let config = ProgramTestConfig::new(
        false, // TODO: enable once cli with new prover server is released
        Some(vec![("account_comparison", account_comparison::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let user = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();

    let (address, _) = derive_address(
        &[b"account", user.pubkey().as_ref()],
        &address_tree_info.tree,
        &account_comparison::ID,
    );

    // Create the counter.
    create_compressed_account(&mut rpc, &user, &address, address_tree_info, name.clone())
        .await
        .unwrap();

    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    let data_account = CompressedAccountData::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();
    assert_eq!(data_account.user, user.pubkey());
    assert_eq!(data_account.name, name);
    assert_eq!(data_account.data, [1u8; 128]);

    update_compressed_account(&mut rpc, &user, &compressed_account, [2u8; 128])
        .await
        .unwrap();

    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    let data_account = CompressedAccountData::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();
    assert_eq!(data_account.user, user.pubkey());
    assert_eq!(data_account.name, name);
    assert_eq!(data_account.data, [2u8; 128]);
}

async fn create_compressed_account<R>(
    rpc: &mut R,
    user: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
    name: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(account_comparison::ID);
    remaining_accounts.add_system_accounts(config);

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

    let output_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;
    let address_tree_info = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees[0];

    let instruction_data = account_comparison::instruction::CreateCompressedAccount {
        proof: rpc_result.proof,
        address_tree_info,
        output_tree_index,
        name,
    };

    let accounts = account_comparison::accounts::CreateCompressedAccount {
        user: user.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: account_comparison::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &user.pubkey(), &[user])
        .await
}

async fn update_compressed_account<R>(
    rpc: &mut R,
    user: &Keypair,
    compressed_account: &CompressedAccount,
    new_data: [u8; 128],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(account_comparison::ID);
    remaining_accounts.add_system_accounts(config);

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_tree_infos = rpc_result.pack_tree_infos(&mut remaining_accounts);

    let compressed_account_data = CompressedAccountData::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();

    let account_meta = CompressedAccountMeta {
        tree_info: packed_tree_infos
            .state_trees
            .as_ref()
            .unwrap()
            .packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_tree_infos
            .state_trees
            .as_ref()
            .unwrap()
            .output_tree_index,
    };

    let instruction_data = account_comparison::instruction::UpdateCompressedAccount {
        proof: rpc_result.proof,
        new_data,
        existing_data: compressed_account_data.data,
        name: compressed_account_data.name,
        account_meta,
    };

    let accounts = account_comparison::accounts::UpdateCompressedAccount {
        user: user.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: account_comparison::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &user.pubkey(), &[user])
        .await
}
