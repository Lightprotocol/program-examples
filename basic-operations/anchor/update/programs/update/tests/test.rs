#![cfg(feature = "test-sbf")]

use std::{process::Command, time::Duration};

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
use update::MyCompressedAccount;

/// Starts `light test-validator` (Solana test validator + Photon indexer + Light
/// prover) with the update program loaded, and returns a [`LightClient`]
/// connected to it once the RPC is responsive.
///
/// Requires the Light CLI (`npm i -g @lightprotocol/zk-compression-cli`) and the
/// program `.so`, which `cargo test-sbf` builds into `target/deploy/update.so`.
async fn start_validator_and_connect() -> LightClient {
    let so_path = format!(
        "{}/../../target/deploy/update.so",
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
            &update::ID.to_string(),
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
        if matches!(rpc.get_indexer_health(None).await, Ok(true)) {
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
async fn test_update() {
    let mut rpc = start_validator_and_connect().await;

    // Fund a fresh payer on the local validator.
    let payer = Keypair::new();
    rpc.airdrop_lamports(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    // Create account first
    let address_tree_info = rpc.get_address_tree_v2();
    let (address, _) = derive_address(
        &[b"message", payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &update::ID,
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
    update_compressed_account(&mut rpc, &payer, account, "Updated message!".to_string())
        .await
        .unwrap();

    let updated_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value
        .unwrap();
    let data = &updated_account.data.as_ref().unwrap().data;
    let updated = MyCompressedAccount::deserialize(&mut &data[..]).unwrap();
    assert_eq!(updated.owner, payer.pubkey());
    assert_eq!(updated.message, "Updated message!");
}

async fn update_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: CompressedAccount,
    new_message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();

    let config = SystemAccountMetaConfig::new(update::ID);
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

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let current_account = MyCompressedAccount::deserialize(
        &mut compressed_account.data.as_ref().unwrap().data.as_slice(),
    )?;

    let instruction_data = update::instruction::UpdateAccount {
        proof: rpc_result.proof,
        current_account,
        account_meta: CompressedAccountMeta {
            tree_info: packed_tree_accounts.packed_tree_infos[0],
            address: compressed_account.address.unwrap(),
            output_state_tree_index: packed_tree_accounts.output_tree_index,
        },
        new_message,
    };

    let accounts = update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: update::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
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
    let config = SystemAccountMetaConfig::new(update::ID);
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

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction_data = update::instruction::CreateAccount {
        proof: rpc_result.proof,
        address_tree_info: packed_accounts.address_trees[0],
        output_state_tree_index,
        message,
    };

    let accounts = update::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: update::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
