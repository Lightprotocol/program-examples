use std::{process::Command, time::Duration};

use account_comparison::CompressedAccountData;
use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use light_client::{
    indexer::{AddressWithTree, CompressedAccount, Indexer, TreeInfo},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig},
};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the account-comparison program loaded, and returns a
/// [`LightClient`] connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into
/// `target/deploy/account_comparison.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/account_comparison.so",
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
            &account_comparison::ID.to_string(),
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
async fn test_create_compressed_account() {
    let name = "Heinrich".to_string();

    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let user = Keypair::new();
    rpc.airdrop_lamports(&user.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    let address_tree_info = rpc.get_address_tree_v2();

    let (address, _) = derive_address(
        &[b"account", user.pubkey().as_ref()],
        &address_tree_info.tree,
        &account_comparison::ID,
    );

    // Create the counter.
    create_compressed_account(&mut rpc, &user, &address, address_tree_info, name.clone())
        .await
        .unwrap();

    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    let data_account = CompressedAccountData::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();
    assert_eq!(data_account.user, user.pubkey());
    assert_eq!(data_account.name, name);
    assert_eq!(data_account.data, [1u8; 128]);

    update_compressed_account(&mut rpc, &user, &compressed_account, [2u8; 128])
        .await
        .unwrap();

    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    let data_account = CompressedAccountData::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();
    assert_eq!(data_account.user, user.pubkey());
    assert_eq!(data_account.name, name);
    assert_eq!(data_account.data, [2u8; 128]);
}

async fn create_compressed_account<R>(
    rpc: &mut R,
    user: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
    name: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(account_comparison::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                tree: address_tree_info.tree,
                address: *address,
            }],
            None,
        )
        .await?
        .value;

    let output_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;
    let address_tree_info = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees[0];

    let instruction_data = account_comparison::instruction::CreateCompressedAccount {
        proof: rpc_result.proof,
        address_tree_info,
        output_tree_index,
        name,
    };

    let accounts = account_comparison::accounts::CreateCompressedAccount {
        user: user.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: account_comparison::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &user.pubkey(), &[user])
        .await
}

async fn update_compressed_account<R>(
    rpc: &mut R,
    user: &Keypair,
    compressed_account: &CompressedAccount,
    new_data: [u8; 128],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(account_comparison::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_tree_infos = rpc_result.pack_tree_infos(&mut remaining_accounts);

    let compressed_account_data = CompressedAccountData::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();

    let account_meta = CompressedAccountMeta {
        tree_info: packed_tree_infos
            .state_trees
            .as_ref()
            .unwrap()
            .packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_tree_infos
            .state_trees
            .as_ref()
            .unwrap()
            .output_tree_index,
    };

    let instruction_data = account_comparison::instruction::UpdateCompressedAccount {
        proof: rpc_result.proof,
        new_data,
        existing_data: compressed_account_data.data,
        name: compressed_account_data.name,
        account_meta,
    };

    let accounts = account_comparison::accounts::UpdateCompressedAccount {
        user: user.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: account_comparison::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &user.pubkey(), &[user])
        .await
}
