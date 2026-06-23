#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use borsh::BorshDeserialize;
use counter::{
    CloseCounterInstructionData, CounterAccount, CreateCounterInstructionData,
    DecrementCounterInstructionData, IncrementCounterInstructionData, ResetCounterInstructionData,
};
use light_client::{
    indexer::{AddressWithTree, CompressedAccount, Indexer, TreeInfo},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::address::v2::derive_address;
use light_sdk::instruction::{
    account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig,
};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the counter program loaded, and returns a [`LightClient`]
/// connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into `target/deploy/counter.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!("{}/target/deploy/counter.so", env!("CARGO_MANIFEST_DIR"));

    // Stop any previously running validator/indexer/prover for a clean slate.
    let _ = Command::new("light")
        .args(["test-validator", "--stop"])
        .status();

    Command::new("light")
        .args([
            "test-validator",
            "--sbf-program",
            &counter::ID.to_string(),
            &so_path,
        ])
        .spawn()
        .expect("failed to launch `light test-validator`; is the Light CLI installed?");

    // Wait until the validator RPC is responsive, then give the indexer and
    // prover a moment to finish initializing.
    let mut rpc = LightClient::new(LightClientConfig::local()).await.unwrap();
    for attempt in 0..60 {
        if rpc.get_slot().await.is_ok() {
            break;
        }
        assert!(attempt < 59, "validator RPC did not come up in time");
        tokio::time::sleep(Duration::from_secs(1)).await;
        rpc = LightClient::new(LightClientConfig::local()).await.unwrap();
    }
    tokio::time::sleep(Duration::from_secs(10)).await;

    rpc
}

// `LightClient` wraps the blocking `solana_rpc_client::RpcClient`, which uses
// `block_in_place` internally and therefore requires a multi-threaded runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_counter() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    let address_tree_info = rpc.get_address_tree_v2();
    let address_tree_pubkey = address_tree_info.tree;

    // Create counter
    let (address, _) = derive_address(
        &[b"counter", payer.pubkey().as_ref()],
        &address_tree_pubkey,
        &counter::ID,
    );
    create_counter(&mut rpc, &payer, &address, address_tree_info)
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
    increment_counter(&mut rpc, &payer, &compressed_counter)
        .await
        .unwrap();

    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Test decrement
    decrement_counter(&mut rpc, &payer, &compressed_counter)
        .await
        .unwrap();

    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Test reset
    reset_counter(&mut rpc, &payer, &compressed_counter)
        .await
        .unwrap();

    let compressed_counter = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    // Test close
    close_counter(&mut rpc, &payer, &compressed_counter)
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

    // NOTE: the validator/indexer/prover are intentionally left running. The
    // `--stop` performed at the start of the next run cleans them up. Stopping
    // them here would tear down processes in this test's process group and kill
    // the test binary itself (SIGKILL) before it can exit cleanly.
}

pub async fn create_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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

    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut accounts)?;
    let packed_address_tree_info = rpc_result.pack_tree_infos(&mut accounts).address_trees[0];
    let (account_metas, _, _) = accounts.to_account_metas();

    let instruction_data = CreateCounterInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index,
    };
    let inputs = borsh::to_vec(&instruction_data).unwrap();

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
        .await
}

pub async fn increment_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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
    let inputs = borsh::to_vec(&instruction_data).unwrap();

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
        .await
}

pub async fn decrement_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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
    let inputs = borsh::to_vec(&instruction_data).unwrap();

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
        .await
}

pub async fn reset_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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
    let inputs = borsh::to_vec(&instruction_data).unwrap();

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
        .await
}

pub async fn close_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(counter::ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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
    let inputs = borsh::to_vec(&instruction_data).unwrap();

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
        .await
}
