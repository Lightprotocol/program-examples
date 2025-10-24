use borsh::{BorshDeserialize, BorshSerialize};
use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::address::v1::derive_address;
use light_sdk::instruction::{
    account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig,
};
use native_program_reinit::{ReinitInstructionData, InstructionType, MyCompressedAccount, ID};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

#[tokio::test]
async fn test_reinit() {
    let config = ProgramTestConfig::new(true, Some(vec![
        ("native_program_reinit", ID),
    ]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();
    let address_tree_pubkey = address_tree_info.tree;

    // Create compressed account
    let (address, _) = derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_pubkey,
        &ID,
    );
    let merkle_tree_pubkey = rpc.get_random_state_tree_info().unwrap().tree;

    native_program_reinit::test_helpers::create_compressed_account(
        &payer,
        &mut rpc,
        &merkle_tree_pubkey,
        address_tree_pubkey,
        address,
        "Hello, compressed world!".to_string(),
    )
    .await
    .unwrap();

    // Get the created account
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Close the account
    native_program_reinit::test_helpers::close_compressed_account(&payer, &mut rpc, &compressed_account)
        .await
        .unwrap();

    // Verify account is closed
    let closed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    assert_eq!(closed_account.data, Some(Default::default()));

    // Reinitialize the account
    reinit_compressed_account(&payer, &mut rpc, &closed_account)
        .await
        .unwrap();

    // Verify account is reinitialized with default MyCompressedAccount values
    let reinit_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Deserialize and verify it's a default-initialized MyCompressedAccount
    let deserialized_account = MyCompressedAccount::deserialize(
        &mut reinit_account.data.as_ref().unwrap().data.as_slice()
    )
    .unwrap();

    // Check that the reinitialized account has default values
    assert_eq!(deserialized_account.owner, Pubkey::default());
    assert_eq!(deserialized_account.message, String::default());
}

pub async fn reinit_compressed_account(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config)?;

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_accounts = rpc_result
        .pack_tree_infos(&mut accounts)
        .state_trees
        .unwrap();

    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (account_metas, _, _) = accounts.to_account_metas();
    let instruction_data = ReinitInstructionData {
        proof: rpc_result.proof,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: ID,
        accounts: account_metas,
        data: [
            &[InstructionType::Reinit as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
