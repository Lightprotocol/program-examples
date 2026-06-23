#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use borsh::BorshDeserialize;
use light_client::{
    indexer::{CompressedAccount, Indexer},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::address::v2::derive_address;
use light_sdk::instruction::{
    account_meta::CompressedAccountMetaBurn, PackedAccounts, SystemAccountMetaConfig,
};
use native_program_burn::{BurnInstructionData, InstructionType, MyCompressedAccount, ID};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the program loaded, and returns a [`LightClient`] connected to it
/// once the RPC is responsive.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/native_program_burn.so",
        env!("CARGO_MANIFEST_DIR")
    );

    // Stop any previously running validator/indexer/prover for a clean slate.
    let _ = Command::new("light")
        .args(["test-validator", "--stop"])
        .status();

    Command::new("light")
        .args([
            "test-validator",
            "--sbf-program",
            &ID.to_string(),
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

    // Wait for the indexer + prover to be ready (not just the validator RPC),
    // otherwise proof requests race the still-initializing indexer/prover.
    for attempt in 0..120 {
        if matches!(rpc.get_indexer_health(Some(light_client::indexer::RetryConfig { num_retries: 0, delay_ms: 0, max_delay_ms: 0 })).await, Ok(true)) {
            break;
        }
        assert!(attempt < 119, "indexer did not become healthy in time");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    rpc
}

// `LightClient` wraps the blocking `solana_rpc_client::RpcClient`, which uses
// `block_in_place` internally and therefore requires a multi-threaded runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_burn() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    let address_tree_info = rpc.get_address_tree_v2();
    let address_tree_pubkey = address_tree_info.tree;

    // Create compressed account
    let (address, _) = derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_pubkey,
        &ID,
    );
    native_program_burn::test_helpers::create_compressed_account(
        &payer,
        &mut rpc,
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

pub async fn burn_compressed_account<R>(
    payer: &Keypair,
    rpc: &mut R,
    compressed_account: &CompressedAccount,
) -> Result<(), RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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

    let current_account = MyCompressedAccount::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();

    println!(
        "Account owner from chain (program): {:?}",
        compressed_account.owner
    );
    println!("Account data owner (user): {:?}", current_account.owner);
    println!("Account message: {:?}", current_account.message);
    println!("Account hash: {:?}", hash);
    println!(
        "Account data bytes: {:?}",
        &compressed_account.data.as_ref().unwrap().data
    );

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
    let inputs = borsh::to_vec(&instruction_data).unwrap();

    let instruction = Instruction {
        program_id: ID,
        accounts: account_metas,
        data: [&[InstructionType::Burn as u8][..], &inputs[..]].concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
