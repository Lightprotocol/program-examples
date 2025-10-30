use borsh::{BorshDeserialize, BorshSerialize};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::address::v1::derive_address;
use light_sdk::instruction::{PackedAccounts, SystemAccountMetaConfig};
use native_program_create::{CreateInstructionData, InstructionType, MyCompressedAccount, ID};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

#[tokio::test]
async fn test_create() {
    let config = ProgramTestConfig::new(true, Some(vec![("native_program_create", ID)]));
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

    create_compressed_account(
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
    assert_eq!(compressed_account.address.unwrap(), address);

    // Deserialize and verify the account data
    let my_account =
        MyCompressedAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();
    assert_eq!(my_account.owner, payer.pubkey());
    assert_eq!(my_account.message, "Hello, compressed world!");
}

pub async fn create_compressed_account(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    merkle_tree_pubkey: &Pubkey,
    address_tree_pubkey: Pubkey,
    address: [u8; 32],
    message: String,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts(system_account_meta_config)?;

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

    let output_state_tree_index = accounts.insert_or_get(*merkle_tree_pubkey);
    let packed_address_tree_info = rpc_result.pack_tree_infos(&mut accounts).address_trees[0];
    let (account_metas, _, _) = accounts.to_account_metas();

    let instruction_data = CreateInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index: output_state_tree_index,
        message,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: ID,
        accounts: account_metas,
        data: [
            &[InstructionType::Create as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
