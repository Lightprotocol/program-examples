use anchor_lang::{InstructionData, ToAccountMetas};
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use nullifier::nullifier_creation::NullifierInstructionData;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signature::Signer};

#[tokio::test]
async fn test_create_single_nullifier() {
    let config = ProgramTestConfig::new(true, Some(vec![("nullifier", nullifier::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let nullifier = Pubkey::new_unique().to_bytes();

    let (data, remaining_accounts) =
        build_create_nullifier_instruction_data(&mut rpc, &[nullifier])
            .await
            .unwrap();

    let instruction_data = nullifier::instruction::CreateNullifier {
        data,
        nullifiers: vec![nullifier],
    };
    let accounts = nullifier::accounts::CreateNullifierAccounts {
        signer: payer.pubkey(),
    };
    let instruction = Instruction {
        program_id: nullifier::ID,
        accounts: [accounts.to_account_metas(None), remaining_accounts].concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    assert_nullifiers_exist(&mut rpc, &[nullifier]).await;

    // Duplicate should fail
    let (dup_data, dup_remaining_accounts) =
        build_create_nullifier_instruction_data(&mut rpc, &[nullifier])
            .await
            .unwrap();

    let dup_instruction_data = nullifier::instruction::CreateNullifier {
        data: dup_data,
        nullifiers: vec![nullifier],
    };
    let dup_accounts = nullifier::accounts::CreateNullifierAccounts {
        signer: payer.pubkey(),
    };
    let dup_instruction = Instruction {
        program_id: nullifier::ID,
        accounts: [dup_accounts.to_account_metas(None), dup_remaining_accounts].concat(),
        data: dup_instruction_data.data(),
    };

    let result = rpc
        .create_and_send_transaction(&[dup_instruction], &payer.pubkey(), &[&payer])
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_multiple_nullifiers() {
    let config = ProgramTestConfig::new(true, Some(vec![("nullifier", nullifier::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let nullifiers: Vec<[u8; 32]> = (0..3).map(|_| Pubkey::new_unique().to_bytes()).collect();

    let (data, remaining_accounts) = build_create_nullifier_instruction_data(&mut rpc, &nullifiers)
        .await
        .unwrap();

    let instruction_data = nullifier::instruction::CreateNullifier {
        data,
        nullifiers: nullifiers.clone(),
    };
    let accounts = nullifier::accounts::CreateNullifierAccounts {
        signer: payer.pubkey(),
    };
    let instruction = Instruction {
        program_id: nullifier::ID,
        accounts: [accounts.to_account_metas(None), remaining_accounts].concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    assert_nullifiers_exist(&mut rpc, &nullifiers).await;
}

async fn assert_nullifiers_exist<R>(rpc: &mut R, nullifiers: &[[u8; 32]])
where
    R: Rpc + Indexer,
{
    use light_sdk::address::v2::derive_address;
    use nullifier::nullifier_creation::NULLIFIER_PREFIX;

    let address_tree_info = rpc.get_address_tree_v2();

    for nullifier in nullifiers {
        let (address, _) = derive_address(
            &[NULLIFIER_PREFIX, nullifier.as_slice()],
            &address_tree_info.tree,
            &nullifier::ID,
        );

        let account = rpc
            .get_compressed_account(address, None)
            .await
            .expect("Failed to fetch compressed account")
            .value;

        assert!(
            account.is_some(),
            "Nullifier account not found for address {:?}",
            address
        );
    }
}

async fn build_create_nullifier_instruction_data<R>(
    rpc: &mut R,
    nullifiers: &[[u8; 32]],
) -> Result<
    (
        NullifierInstructionData,
        Vec<solana_sdk::instruction::AccountMeta>,
    ),
    RpcError,
>
where
    R: Rpc + Indexer,
{
    use light_program_test::AddressWithTree;
    use light_sdk::{
        address::v2::derive_address,
        instruction::{PackedAccounts, SystemAccountMetaConfig},
    };
    use nullifier::nullifier_creation::NULLIFIER_PREFIX;

    let address_tree_info = rpc.get_address_tree_v2();

    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(nullifier::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let address_with_trees: Vec<AddressWithTree> = nullifiers
        .iter()
        .map(|n| {
            let (address, _) = derive_address(
                &[NULLIFIER_PREFIX, n.as_slice()],
                &address_tree_info.tree,
                &nullifier::ID,
            );
            AddressWithTree {
                address,
                tree: address_tree_info.tree,
            }
        })
        .collect();

    let rpc_result = rpc
        .get_validity_proof(vec![], address_with_trees, None)
        .await?
        .value;

    let packed_address_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees;

    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let (remaining_accounts_metas, system_accounts_offset, _) =
        remaining_accounts.to_account_metas();

    let data = NullifierInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        system_accounts_offset: system_accounts_offset as u8,
    };

    Ok((data, remaining_accounts_metas))
}
