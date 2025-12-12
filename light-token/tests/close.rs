use borsh::BorshDeserialize;
use light_client::indexer::{AddressWithTree, Indexer};
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::ctoken::{
    CloseCTokenAccount, CreateCMint, CreateCMintParams, CreateCTokenAccount, CTOKEN_PROGRAM_ID,
};
use light_ctoken_interface::state::CToken;
use serde_json;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::convert::TryFrom;
use std::env;
use std::fs;

#[tokio::test(flavor = "multi_thread")]
async fn test_close_ctoken_account() {
    dotenvy::dotenv().ok();

    let keypair_path = env::var("KEYPAIR_PATH")
        .unwrap_or_else(|_| format!("{}/.config/solana/id.json", env::var("HOME").unwrap()));
    let payer = load_keypair(&keypair_path).expect("Failed to load keypair");

    let api_key = env::var("api_key")
        .expect("api_key environment variable must be set");

    let config = LightClientConfig::devnet(
        Some("https://devnet.helius-rpc.com".to_string()),
        Some(api_key),
    );
    let mut rpc = LightClient::new_with_retry(config, None)
        .await
        .expect("Failed to initialize LightClient");

    // Step 1: Create compressed mint (prerequisite)
    let (mint, _compression_address) = create_compressed_mint(&mut rpc, &payer, 9).await;

    // Step 2: Create cToken account with 0 balance
    let account = Keypair::new();
    let owner = payer.pubkey();

    let create_instruction =
        CreateCTokenAccount::new(payer.pubkey(), account.pubkey(), mint, owner)
            .instruction()
            .unwrap();

    rpc.create_and_send_transaction(&[create_instruction], &payer.pubkey(), &[&payer, &account])
        .await
        .unwrap();

    // Step 3: Verify account exists before closing
    let account_before_close = rpc.get_account(account.pubkey()).await.unwrap();
    assert!(
        account_before_close.is_some(),
        "Account should exist before closing"
    );

    let ctoken_state =
        CToken::deserialize(&mut &account_before_close.unwrap().data[..]).unwrap();
    println!("\ncToken account state before closing:");
    println!("  Mint: {:?}", ctoken_state.mint);
    println!("  Owner: {:?}", ctoken_state.owner);
    println!("  Amount: {}", ctoken_state.amount);
    assert_eq!(ctoken_state.amount, 0, "Account balance must be 0 to close");

    // Step 4: Build close instruction using SDK builder
    let close_instruction = CloseCTokenAccount::new(
        CTOKEN_PROGRAM_ID,
        account.pubkey(),
        payer.pubkey(), // Destination for remaining lamports
        owner,
    )
    .instruction()
    .unwrap();

    // Step 5: Send close transaction
    let close_tx_sig = rpc
        .create_and_send_transaction(&[close_instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    println!("\nClose transaction: {}", close_tx_sig);

    // Step 6: Verify account is closed
    let account_after_close = rpc.get_account(account.pubkey()).await.unwrap();
    assert!(
        account_after_close.is_none(),
        "Account should be closed and no longer exist"
    );

    println!("Successfully closed cToken account!");
}

pub async fn create_compressed_mint<R: Rpc + Indexer>(
    rpc: &mut R,
    payer: &Keypair,
    decimals: u8,
) -> (Pubkey, [u8; 32]) {
    let mint_signer = Keypair::new();
    let address_tree = rpc.get_address_tree_v2();

    // Fetch active state trees for devnet
    let _ = rpc.get_latest_active_state_trees().await;
    let output_pubkey = match rpc
        .get_random_state_tree_info()
        .ok()
        .or_else(|| rpc.get_random_state_tree_info_v1().ok())
    {
        Some(info) => info
            .get_output_pubkey()
            .expect("Invalid state tree type for output"),
        None => {
            let queues = rpc
                .indexer_mut()
                .expect("IndexerNotInitialized")
                .get_queue_info(None)
                .await
                .expect("Failed to fetch queue info")
                .value
                .queues;
            queues
                .get(0)
                .map(|q| q.queue)
                .expect("NoStateTreesAvailable: no active state trees returned")
        }
    };

    // Derive compression address
    let compression_address = light_ctoken_sdk::ctoken::derive_cmint_compressed_address(
        &mint_signer.pubkey(),
        &address_tree.tree,
    );

    let mint_pda = light_ctoken_sdk::ctoken::find_cmint_address(&mint_signer.pubkey()).0;

    // Get validity proof for the address
    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: compression_address,
                tree: address_tree.tree,
            }],
            None,
        )
        .await
        .unwrap()
        .value;

    // Build params
    let params = CreateCMintParams {
        decimals,
        address_merkle_tree_root_index: rpc_result.addresses[0].root_index,
        mint_authority: payer.pubkey(),
        proof: rpc_result.proof.0.unwrap(),
        compression_address,
        mint: mint_pda,
        freeze_authority: None,
        extensions: None,
    };

    // Create instruction
    let create_cmint = CreateCMint::new(
        params,
        mint_signer.pubkey(),
        payer.pubkey(),
        address_tree.tree,
        output_pubkey,
    );
    let instruction = create_cmint.instruction().unwrap();

    // Send transaction
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer, &mint_signer])
        .await
        .unwrap();

    (mint_pda, compression_address)
}

fn load_keypair(path: &str) -> Result<Keypair, Box<dyn std::error::Error>> {
    let path = if path.starts_with("~") {
        path.replace("~", &env::var("HOME").unwrap_or_default())
    } else {
        path.to_string()
    };
    let file = fs::read_to_string(&path)?;
    let bytes: Vec<u8> = serde_json::from_str(&file)?;
    Ok(Keypair::try_from(&bytes[..])?)
}
