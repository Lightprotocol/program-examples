use borsh::{BorshDeserialize, BorshSerialize};
use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::address::v1::derive_address;
use light_sdk::instruction::{
    account_meta::CompressedAccountMetaBurn, PackedAccounts, SystemAccountMetaConfig,
};
use native_program_burn::{BurnInstructionData, InstructionType, MyCompressedAccount, ID};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signer},
};

#[tokio::test]
async fn test_burn() {
    let config = ProgramTestConfig::new(true, Some(vec![
        ("native_program_burn", ID),
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

    native_program_burn::test_helpers::create_compressed_account(
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
    println!("compressed_account: {:?}", compressed_account);
    assert_eq!(compressed_account.address.unwrap(), address);

    // Burn the account
    burn_compressed_account(&payer, &mut rpc, &compressed_account)
        .await
        .unwrap();

    // Verify account is burned (should be None)
    let burned_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    assert!(burned_account.is_none());
}

pub async fn burn_compressed_account(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config)?;

    let hash = compressed_account.hash;

    println!("Requesting proof for hash: {:?}", hash);

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    println!("Proof returned for hashes: {:?}", rpc_result.proof);

    let packed_accounts = rpc_result
        .pack_tree_infos(&mut accounts)
        .state_trees
        .unwrap();

    let current_account =
        MyCompressedAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    println!("Account owner from chain (program): {:?}", compressed_account.owner);
    println!("Account data owner (user): {:?}", current_account.owner);
    println!("Account message: {:?}", current_account.message);
    println!("Account hash: {:?}", hash);
    println!("Account data bytes: {:?}", &compressed_account.data.as_ref().unwrap().data);

    let meta = CompressedAccountMetaBurn {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
    };

    let (account_metas, _, _) = accounts.to_account_metas();
    let instruction_data = BurnInstructionData {
        proof: rpc_result.proof,
        account_meta: meta,
        current_account,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: ID,
        accounts: account_metas,
        data: [
            &[InstructionType::Burn as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
