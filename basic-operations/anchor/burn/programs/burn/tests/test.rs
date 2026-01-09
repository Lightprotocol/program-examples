#![cfg(feature = "test-sbf")]

use anchor_lang::AnchorDeserialize;
use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{account_meta::CompressedAccountMetaBurn, PackedAccounts, SystemAccountMetaConfig},
};
use burn::MyCompressedAccount;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signature, Signer},
};

#[tokio::test]
async fn test_burn() {
    let config = ProgramTestConfig::new(true, Some(vec![
        ("burn", burn::ID),
    ]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // Create account first
    let address_tree_info = rpc.get_address_tree_v2();
    let (address, _) = derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &burn::ID,
    );

    create_compressed_account(
        &mut rpc,
        &payer,
        &address,
        "Hello, compressed world!".to_string(),
    )
    .await
    .unwrap();

    let account = rpc.get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    let data = &account.data.as_ref().unwrap().data;
    let message_account = MyCompressedAccount::deserialize(&mut &data[..]).unwrap();
    assert_eq!(message_account.owner, payer.pubkey());
    assert_eq!(message_account.message, "Hello, compressed world!");

    // Burn the account
    burn_compressed_account(&mut rpc, &payer, account, "Hello, compressed world!".to_string())
        .await
        .unwrap();

    // Verify account is burned (should not exist)
    let result = rpc.get_compressed_account(address, None).await;
    assert!(result.unwrap().value.is_none(), "Account should be burned and not exist");
}

async fn burn_compressed_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    compressed_account: CompressedAccount,
    current_message: String,
) -> Result<Signature, RpcError> {
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(burn::ID);
    remaining_accounts.add_system_accounts_v2(config)?;
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

    let instruction = Instruction {
        program_id: burn::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            burn::instruction::BurnAccount {
                proof: rpc_result.proof,
                account_meta: CompressedAccountMetaBurn {
                    tree_info: packed_tree_accounts.packed_tree_infos[0],
                    address: compressed_account.address.unwrap(),
                },
                current_message,
            }
            .data()
        },
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn create_compressed_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    address: &[u8; 32],
    message: String,
) -> Result<Signature, RpcError> {
    let config = SystemAccountMetaConfig::new(burn::ID);
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_system_accounts_v2(config)?;

    let address_tree_info = rpc.get_address_tree_v2();

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![light_program_test::AddressWithTree {
                address: *address,
                tree: address_tree_info.tree,
            }],
            None,
        )
        .await?
        .value;
    let packed_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);

    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let (remaining_accounts, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: burn::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            burn::instruction::CreateAccount {
                proof: rpc_result.proof,
                address_tree_info: packed_accounts.address_trees[0],
                output_state_tree_index: output_state_tree_index,
                message,
            }
            .data()
        },
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
