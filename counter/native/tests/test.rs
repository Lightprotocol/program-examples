#![cfg(feature = "test-sbf")]

use borsh::{BorshDeserialize, BorshSerialize};
use counter::{
    CloseCounterInstructionData, CounterAccount, CreateCounterInstructionData,
    DecrementCounterInstructionData, IncrementCounterInstructionData, ResetCounterInstructionData,
};
use light_client::indexer::CompressedAccount;
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::address::v1::derive_address;
use light_sdk::instruction::{
    account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig,
};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

#[tokio::test]
async fn test_counter() {
    let config = ProgramTestConfig::new(true, Some(vec![("counter", counter::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();
    let address_tree_pubkey = address_tree_info.tree;

    // Create counter
    let (address, _) = derive_address(
        &[b"counter", payer.pubkey().as_ref()],
        &address_tree_pubkey,
        &counter::ID,
    );
    let merkle_tree_pubkey = rpc.get_random_state_tree_info().unwrap().tree;

    create_counter(
        &payer,
        &mut rpc,
        &merkle_tree_pubkey,
        address_tree_pubkey,
        address,
    )
    .await
    .unwrap();

    // Get the created counter
    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    assert_eq!(compressed_counter.address.unwrap(), address);

    // Test increment
    increment_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Test decrement
    decrement_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Test reset
    reset_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Test close
    close_counter(&payer, &mut rpc, &compressed_counter)
        .await
        .unwrap();

    // Check that it was closed correctly (account data should be default).
    let closed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    assert_eq!(closed_account.data, Some(Default::default()));
}

pub async fn create_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    merkle_tree_pubkey: &Pubkey,
    address_tree_pubkey: Pubkey,
    address: [u8; 32],
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
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

    let output_merkle_tree_index = accounts.insert_or_get(*merkle_tree_pubkey);
    let packed_address_tree_info = rpc_result.pack_tree_infos(&mut accounts).address_trees[0];
    let (account_metas, _, _) = accounts.to_account_metas();

    let instruction_data = CreateCounterInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index: output_merkle_tree_index,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: account_metas,
        data: [
            &[counter::InstructionType::CreateCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

pub async fn increment_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
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

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (account_metas, _, _) = accounts.to_account_metas();
    let instruction_data = IncrementCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: account_metas,
        data: [
            &[counter::InstructionType::IncrementCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

pub async fn decrement_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
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

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (account_metas, _, _) = accounts.to_account_metas();
    let instruction_data = DecrementCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: account_metas,
        data: [
            &[counter::InstructionType::DecrementCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

pub async fn reset_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
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

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let meta = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (account_metas, _, _) = accounts.to_account_metas();
    let instruction_data = ResetCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: account_metas,
        data: [
            &[counter::InstructionType::ResetCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}

pub async fn close_counter(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
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

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let meta_close = CompressedAccountMeta {
        tree_info: packed_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_accounts.output_tree_index,
    };

    let (account_metas, _, _) = accounts.to_account_metas();
    let instruction_data = CloseCounterInstructionData {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta: meta_close,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: account_metas,
        data: [
            &[counter::InstructionType::CloseCounter as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
