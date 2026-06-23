#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use light_client::{
    indexer::{AddressWithTree, CompressedAccount, Indexer, TreeInfo},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{
        account_meta::CompressedAccountMetaReadOnly, PackedAccounts, SystemAccountMetaConfig,
    },
};
use read_only::{DataAccount, ExistingCompressedAccountIxData, FIRST_SEED};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the read-only program loaded, and returns a [`LightClient`]
/// connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into `target/deploy/read_only.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/target/deploy/read_only.so",
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
            &read_only::ID.to_string(),
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

/// Wait until the indexer has processed up to the current chain slot, so that
/// reads after a mutating transaction reflect the new state (avoids stale reads).
async fn wait_for_indexer_catchup(rpc: &LightClient) {
    let target = rpc.get_slot().await.unwrap_or(0);
    for _ in 0..60 {
        if rpc
            .get_indexer_slot(Some(light_client::indexer::RetryConfig {
                num_retries: 0,
                delay_ms: 0,
                max_delay_ms: 0,
            }))
            .await
            .map(|s| s >= target)
            .unwrap_or(false)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// `LightClient` wraps the blocking `solana_rpc_client::RpcClient`, which uses
// `block_in_place` internally and therefore requires a multi-threaded runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_read_compressed_account() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    let address_tree_info = rpc.get_address_tree_v2();

    let (address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &read_only::ID,
    );

    // Create the compressed account
    create_compressed_account(
        &mut rpc,
        &payer,
        &address,
        address_tree_info,
        "Hello, World!".to_string(),
    )
    .await
    .unwrap();
    wait_for_indexer_catchup(&rpc).await;

    // Check that it was created correctly
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    assert_eq!(compressed_account.leaf_index, 0);
    let data = &compressed_account.data.as_ref().unwrap().data;
    let account_data = DataAccount::deserialize(&mut &data[..]).unwrap();
    assert_eq!(account_data.owner, payer.pubkey());
    assert_eq!(account_data.message, "Hello, World!");

    // Test reading the compressed account
    read_compressed_account(
        &mut rpc,
        &payer,
        &compressed_account,
        "Hello, World!".to_string(),
    )
    .await
    .unwrap();

    // NOTE: the validator/indexer/prover are intentionally left running. The
    // `--stop` performed at the start of the next run cleans them up. Stopping
    // them here would tear down processes in this test's process group and kill
    // the test binary itself (SIGKILL) before it can exit cleanly.
}

async fn create_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(read_only::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

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

    let packed_address_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees;
    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let instruction_data = read_only::instruction::CreateCompressedAccount {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        message,
    };
    let accounts = read_only::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();
    let instruction = Instruction {
        program_id: read_only::ID,
        accounts: [accounts.to_account_metas(None), remaining_accounts_metas].concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn read_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(read_only::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let hash = compressed_account.hash;
    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();

    let account_meta = CompressedAccountMetaReadOnly {
        tree_info: packed_state_tree_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
    };

    let instruction_data = read_only::instruction::Read {
        proof: rpc_result.proof,
        existing_account: ExistingCompressedAccountIxData {
            account_meta,
            message,
        },
    };

    let accounts = read_only::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();
    let instruction = Instruction {
        program_id: read_only::ID,
        accounts: [accounts.to_account_metas(None), remaining_accounts_metas].concat(),
        data: instruction_data.data(),
    };
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
