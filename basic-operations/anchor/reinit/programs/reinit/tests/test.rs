#![cfg(feature = "test-sbf")]

use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::instruction::{
    account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig,
};
use light_sdk::LightDiscriminator;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signature, Signer},
};

#[tokio::test]
async fn test_reinit() {
    let config = ProgramTestConfig::new(true, Some(vec![
        ("reinit", reinit::ID),
    ]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();
    let (address, _) = light_sdk::address::v1::derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &reinit::ID,
    );

    create_compressed_account(
        &mut rpc,
        &payer,
        &address,
        "Hello, compressed world!".to_string(),
    )
    .await
    .unwrap();

    let account = get_compressed_account(&mut rpc, address).await;
    close_compressed_account(&mut rpc, &payer, account, "Hello, compressed world!".to_string())
        .await
        .unwrap();

    let closed = rpc.get_compressed_account(address, None).await.unwrap().value.unwrap();
    assert_eq!(closed.address.as_ref().unwrap(), &address);
    assert_eq!(closed.owner, reinit::ID);

    let data = closed.data.as_ref().unwrap();
    assert_eq!(data.discriminator, [0u8; 8]);
    assert!(data.data.is_empty());
    assert_eq!(data.data_hash, [0u8; 32]);

    // Reinitialize the closed account
    reinit_compressed_account(&mut rpc, &payer, closed)
        .await
        .unwrap();

    // Verify reinitialized account has default values
    let reinitialized = rpc.get_compressed_account(address, None).await.unwrap().value.unwrap();
    assert_eq!(reinitialized.address.as_ref().unwrap(), &address);
    assert_eq!(reinitialized.owner, reinit::ID);

    let data = reinitialized.data.as_ref().unwrap();
    // Default MyCompressedAccount should have empty message and default pubkey
    assert_eq!(data.discriminator, reinit::MyCompressedAccount::LIGHT_DISCRIMINATOR);
    assert!(!data.data.is_empty()); // Has default-initialized data now
}

async fn close_compressed_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    compressed_account: CompressedAccount,
    message: String,
) -> Result<Signature, RpcError> {
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(reinit::ID);
    remaining_accounts.add_system_accounts(config)?;
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
        program_id: reinit::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            reinit::instruction::CloseAccount {
                proof: rpc_result.proof,
                account_meta: CompressedAccountMeta {
                    tree_info: packed_tree_accounts.packed_tree_infos[0],
                    address: compressed_account.address.unwrap(),
                    output_state_tree_index: packed_tree_accounts.output_tree_index,
                },
                current_message: message,
            }
            .data()
        },
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn reinit_compressed_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    compressed_account: CompressedAccount,
) -> Result<Signature, RpcError> {
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(reinit::ID);
    remaining_accounts.add_system_accounts(config)?;
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
        program_id: reinit::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            reinit::instruction::ReinitAccount {
                proof: rpc_result.proof,
                account_meta: CompressedAccountMeta {
                    tree_info: packed_tree_accounts.packed_tree_infos[0],
                    address: compressed_account.address.unwrap(),
                    output_state_tree_index: packed_tree_accounts.output_tree_index,
                },
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
        .unwrap()
}

async fn create_compressed_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    address: &[u8; 32],
    message: String,
) -> Result<Signature, RpcError> {
    let config = SystemAccountMetaConfig::new(reinit::ID);
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_system_accounts(config)?;

    let address_tree_info = rpc.get_address_tree_v1();

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
        .get_random_state_tree_info()
        .unwrap()
        .pack_output_tree_index(&mut remaining_accounts)
        .unwrap();

    let (remaining_accounts, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: reinit::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            reinit::instruction::CreateAccount {
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
