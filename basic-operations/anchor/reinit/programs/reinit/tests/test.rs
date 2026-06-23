#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use light_client::{
    indexer::{AddressWithTree, CompressedAccount, Indexer, TreeInfo},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig},
};
use light_sdk::LightDiscriminator;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the reinit program loaded, and returns a [`LightClient`]
/// connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into `target/deploy/reinit.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/reinit.so",
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
            &reinit::ID.to_string(),
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
async fn test_reinit() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    let address_tree_info = rpc.get_address_tree_v2();
    let (address, _) = derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &reinit::ID,
    );

    create_compressed_account(
        &mut rpc,
        &payer,
        &address,
        address_tree_info,
        "Hello, compressed world!".to_string(),
    )
    .await
    .unwrap();

    let account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    close_compressed_account(
        &mut rpc,
        &payer,
        account,
        "Hello, compressed world!".to_string(),
    )
    .await
    .unwrap();

    // Wait for the indexer to index the close before reading it back.
    wait_for_indexer_catchup(&rpc).await;

    let closed = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
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
    let reinitialized = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    assert_eq!(reinitialized.address.as_ref().unwrap(), &address);
    assert_eq!(reinitialized.owner, reinit::ID);

    let data = reinitialized.data.as_ref().unwrap();
    // Default MyCompressedAccount should have empty message and default pubkey
    assert_eq!(
        data.discriminator,
        reinit::MyCompressedAccount::LIGHT_DISCRIMINATOR
    );
    assert!(!data.data.is_empty()); // Has default-initialized data now
}

async fn close_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: CompressedAccount,
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(reinit::ID);
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

async fn reinit_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(reinit::ID);
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
    let config = SystemAccountMetaConfig::new(reinit::ID);
    let mut remaining_accounts = PackedAccounts::default();
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
    let packed_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);

    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

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
                output_state_tree_index,
                message,
            }
            .data()
        },
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
