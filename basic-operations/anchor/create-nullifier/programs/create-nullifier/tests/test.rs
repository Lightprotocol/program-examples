#![cfg(feature = "test-sbf")]

use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{PackedAccounts, SystemAccountMetaConfig},
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signature, Signer},
};

#[tokio::test]
async fn test_create_nullifier() {
    let config = ProgramTestConfig::new(true, Some(vec![("create_nullifier", create_nullifier::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    // Create a 32-byte id
    let id: [u8; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    ];

    let (address, _) = derive_address(
        &[b"nullifier", &id],
        &address_tree_info.tree,
        &create_nullifier::ID,
    );

    create_nullifier_account(&mut rpc, &payer, &address, id)
        .await
        .unwrap();

    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Account should exist but have no data (empty struct)
    assert!(
        compressed_account.data.is_none() || compressed_account.data.as_ref().unwrap().data.is_empty(),
        "Nullifier account should be empty"
    );
}

#[tokio::test]
async fn test_create_nullifier_duplicate_fails() {
    let config = ProgramTestConfig::new(true, Some(vec![("create_nullifier", create_nullifier::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    let id: [u8; 32] = [42u8; 32];

    let (address, _) = derive_address(
        &[b"nullifier", &id],
        &address_tree_info.tree,
        &create_nullifier::ID,
    );

    // First creation should succeed
    create_nullifier_account(&mut rpc, &payer, &address, id)
        .await
        .unwrap();

    // Second creation with same id should fail (address already exists)
    let result = create_nullifier_account(&mut rpc, &payer, &address, id).await;
    assert!(result.is_err(), "Duplicate nullifier creation should fail");
}

async fn create_nullifier_account(
    rpc: &mut LightProgramTest,
    payer: &Keypair,
    address: &[u8; 32],
    id: [u8; 32],
) -> Result<Signature, RpcError> {
    let config = SystemAccountMetaConfig::new(create_nullifier::ID);
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_system_accounts_v2(config)?;

    let address_tree_info = rpc.get_address_tree_v2();

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
    let packed_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);

    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let (remaining_accounts, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: create_nullifier::ID,
        accounts: [
            vec![AccountMeta::new(payer.pubkey(), true)],
            remaining_accounts,
        ]
        .concat(),
        data: {
            use anchor_lang::InstructionData;
            create_nullifier::instruction::CreateAccount {
                proof: rpc_result.proof,
                address_tree_info: packed_accounts.address_trees[0],
                output_state_tree_index,
                id,
            }
            .data()
        },
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
