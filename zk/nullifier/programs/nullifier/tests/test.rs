#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use anchor_lang::{InstructionData, ToAccountMetas};
use light_client::{
    indexer::{AddressWithTree, Indexer},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use nullifier::nullifier_creation::NullifierInstructionData;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the nullifier program loaded, and returns a [`LightClient`]
/// connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into `target/deploy/nullifier.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/nullifier.so",
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
            &nullifier::ID.to_string(),
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
async fn test_create_single_nullifier() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

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

    // Duplicate should fail: with a real indexer, requesting a non-inclusion
    // validity proof for an address that already exists returns an error, so the
    // duplicate is rejected at proof-building time. (The in-process test indexer
    // surfaced this only when the transaction executed.)
    let dup_result = build_create_nullifier_instruction_data(&mut rpc, &[nullifier]).await;
    assert!(
        dup_result.is_err(),
        "expected duplicate nullifier creation to be rejected"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_create_multiple_nullifiers() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

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
) -> Result<(NullifierInstructionData, Vec<AccountMeta>), RpcError>
where
    R: Rpc + Indexer,
{
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
