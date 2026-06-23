#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use create_and_update::{ByteDataAccount, DataAccount, FIRST_SEED, SECOND_SEED};
use light_client::{
    indexer::{AddressWithTree, Indexer, TreeInfo},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{PackedAccounts, SystemAccountMetaConfig},
};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signature::Signature;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the create-and-update program loaded, and returns a
/// [`LightClient`] connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into
/// `target/deploy/create_and_update.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/create_and_update.so",
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
            &create_and_update::ID.to_string(),
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
async fn test_create_two_accounts() {
    let mut rpc = start_validator_and_connect().await;

    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    let address_tree_info = rpc.get_address_tree_v2();

    let (first_address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let (second_address, _) = derive_address(
        &[SECOND_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &create_and_update::ID,
    );

    let byte_data = [1u8; 31]; // 31 bytes of data
    let message = "String account message".to_string();

    // Create two compressed accounts in a single instruction
    create_two_accounts(
        &mut rpc,
        &payer,
        &first_address,
        &second_address,
        address_tree_info,
        byte_data,
        message.clone(),
    )
    .await
    .unwrap();

    // Check that the first account (ByteDataAccount) was created correctly
    let first_compressed_account = rpc
        .get_compressed_account(first_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    let first_data = &first_compressed_account.data.as_ref().unwrap().data;
    let first_account_data = ByteDataAccount::deserialize(&mut &first_data[..]).unwrap();
    assert_eq!(first_account_data.owner, payer.pubkey());
    assert_eq!(first_account_data.data, byte_data);

    // Check that the second account (DataAccount) was created correctly
    let second_compressed_account = rpc
        .get_compressed_account(second_address, None)
        .await
        .unwrap()
        .value
        .unwrap();

    let second_data = &second_compressed_account.data.as_ref().unwrap().data;
    let second_account_data = DataAccount::deserialize(&mut &second_data[..]).unwrap();
    assert_eq!(second_account_data.owner, payer.pubkey());
    assert_eq!(second_account_data.message, message);
}

#[allow(clippy::too_many_arguments)]
async fn create_two_accounts<R>(
    rpc: &mut R,
    payer: &Keypair,
    first_address: &[u8; 32],
    second_address: &[u8; 32],
    address_tree_info: TreeInfo,
    byte_data: [u8; 31],
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(create_and_update::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![
                AddressWithTree {
                    address: *first_address,
                    tree: address_tree_info.tree,
                },
                AddressWithTree {
                    address: *second_address,
                    tree: address_tree_info.tree,
                },
            ],
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

    let instruction_data = create_and_update::instruction::CreateTwoAccounts {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        byte_data,
        message,
    };

    let accounts = create_and_update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: create_and_update::ID,
        accounts: [accounts.to_account_metas(None), {
            let (metas, _, _) = remaining_accounts.to_account_metas();
            metas
        }]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
