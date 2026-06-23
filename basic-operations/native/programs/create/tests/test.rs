#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

use borsh::BorshDeserialize;
use light_client::{
    indexer::{AddressWithTree, Indexer},
    rpc::{LightClient, LightClientConfig, Rpc, RpcError},
};
use light_sdk::address::v2::derive_address;
use light_sdk::instruction::{PackedAccounts, SystemAccountMetaConfig};
use native_program_create::{CreateInstructionData, InstructionType, MyCompressedAccount, ID};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the program loaded, and returns a [`LightClient`] connected to it
/// once the RPC is responsive.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/native_program_create.so",
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
    tokio::time::sleep(Duration::from_secs(10)).await;

    rpc
}

// `LightClient` wraps the blocking `solana_rpc_client::RpcClient`, which uses
// `block_in_place` internally and therefore requires a multi-threaded runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_create() {
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

    create_compressed_account(
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
    assert_eq!(compressed_account.address.unwrap(), address);

    // Deserialize and verify the account data
    let my_account = MyCompressedAccount::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )
    .unwrap();
    assert_eq!(my_account.owner, payer.pubkey());
    assert_eq!(my_account.message, "Hello, compressed world!");
}

pub async fn create_compressed_account<R>(
    payer: &Keypair,
    rpc: &mut R,
    address_tree_pubkey: Pubkey,
    address: [u8; 32],
    message: String,
) -> Result<(), RpcError>
where
    R: Rpc + Indexer,
{
    let system_account_meta_config = SystemAccountMetaConfig::new(ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

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

    let packed_address_tree_info = rpc_result.pack_tree_infos(&mut accounts).address_trees[0];
    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut accounts)?;
    let (account_metas, _, _) = accounts.to_account_metas();

    let instruction_data = CreateInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index,
        message,
    };
    let inputs = borsh::to_vec(&instruction_data).unwrap();

    let instruction = Instruction {
        program_id: ID,
        accounts: account_metas,
        data: [&[InstructionType::Create as u8][..], &inputs[..]].concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
