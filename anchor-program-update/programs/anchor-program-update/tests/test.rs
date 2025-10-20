#![cfg(feature = "test-sbf")]

use anchor_lang::AnchorDeserialize;
use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::instruction::{
    account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig,
};
use anchor_program_update::MyCompressedAccount;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signature, Signer},
};

#[tokio::test]
async fn test_update() {
    let config = ProgramTestConfig::new(true, Some(vec![
        ("program_create", program_create::ID),
        ("anchor_program_update", anchor_program_update::ID),
    ]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // Create account first using create program
    let address_tree_info = rpc.get_address_tree_v1();
    let (address, _) = light_sdk::address::v1::derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &anchor_program_update::ID,
    );

    program_create::tests::create_message_account(
        &mut rpc,
        &payer,
        &address,
        "Hello, compressed world!".to_string(),
    )
    .await
    .unwrap();

    let account = get_compressed_account(&mut rpc, address).await;
    update_message_account(&mut rpc, &payer, account, "Updated message!".to_string())
        .await
        .unwrap();

    let updated = get_message_account(&mut rpc, address).await;
    assert_eq!(updated.owner, payer.pubkey());
    assert_eq!(updated.message, "Updated message!");
}

async fn update_message_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    compressed_account: CompressedAccount,
    new_message: String,
) -> Result<Signature, RpcError> {
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(anchor_program_update::ID);
    remaining_accounts.add_system_accounts(config);
    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .state_trees
        .unwrap();

    let (remaining_accounts, _, _) = remaining_accounts.to_account_metas();

    let current_account = MyCompressedAccount::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();

    let instruction = Instruction {
        program_id: anchor_program_update::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            anchor_program_update::instruction::Update {
                proof: rpc_result.proof,
                account_meta: CompressedAccountMeta {
                    tree_info: packed_tree_accounts.packed_tree_infos[0],
                    address: compressed_account.address.unwrap(),
                    output_state_tree_index: packed_tree_accounts.output_tree_index,
                },
                current_message: current_account.message,
                new_message,
            }
            .data()
        },
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn get_compressed_account(
    rpc: &mut LightProgramTest,
    address: [u8; 32],
) -> CompressedAccount {
    rpc.get_compressed_account(address, None)
        .await
        .unwrap()
        .value
}

async fn get_message_account(
    rpc: &mut LightProgramTest,
    address: [u8; 32],
) -> MyCompressedAccount {
    let account = get_compressed_account(rpc, address).await;
    let data = &account.data.as_ref().unwrap().data;
    MyCompressedAccount::deserialize(&mut &data[..]).unwrap()
}
