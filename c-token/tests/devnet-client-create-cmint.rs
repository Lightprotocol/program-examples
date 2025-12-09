// Test for: client-create-cmint.mdx

use light_client::indexer::{AddressWithTree, Indexer};
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::ctoken::{CreateCMint, CreateCMintParams};
use light_ctoken_interface::instructions::extensions::token_metadata::TokenMetadataInstructionData;
use light_ctoken_interface::instructions::extensions::ExtensionInstructionData;
use light_ctoken_interface::state::AdditionalMetadata;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::env;
use std::fs;
use serde_json;
use std::convert::TryFrom;


#[tokio::test(flavor = "multi_thread")]
async fn test_create_rent_free_mint_with_metadata() {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Initialize LightClient on devnet with Photon indexer
    let keypair_path = env::var("KEYPAIR_PATH")
        .unwrap_or_else(|_| format!("{}/.config/solana/id.json", env::var("HOME").unwrap()));
    let payer = load_keypair(&keypair_path).expect("Failed to load keypair");

    // Get Helius API key from environment
    let helius_api_key = env::var("HELIUS_API_KEY")
        .expect("HELIUS_API_KEY environment variable must be set. Create a .env file or set it in your environment.");

    let photon_base = "https://devnet.helius-rpc.com".to_string();
    let config = LightClientConfig::devnet(Some(photon_base), Some(helius_api_key));
    let mut rpc = LightClient::new_with_retry(config, None)
        .await
        .expect("Failed to initialize LightClient");

    // Create c-mint with metadata
    let (mint, _compression_address) = create_compressed_mint(&mut rpc, &payer, 9).await;

    println!("Created compressed mint: {}", mint);

}
pub async fn create_compressed_mint<R: Rpc + Indexer>(
    rpc: &mut R,
    payer: &Keypair,
    decimals: u8,
) -> (Pubkey, [u8; 32]) {
    let mint_signer = Keypair::new();
    let address_tree = rpc.get_address_tree_v2();
    // Ensure state trees are fetched, then pick a valid one via helper
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

    // Derive address
    let compression_address = light_ctoken_sdk::ctoken::derive_cmint_compressed_address(
        &mint_signer.pubkey(),
        &address_tree.tree,
    );

    let mint_pda =
        light_ctoken_sdk::ctoken::find_cmint_address(&mint_signer.pubkey()).0;

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

    // Build params with token metadata
    let params = CreateCMintParams {
        decimals,
        address_merkle_tree_root_index: rpc_result.addresses[0].root_index,
        mint_authority: payer.pubkey(),
        proof: rpc_result.proof.0.unwrap(),
        compression_address,
        mint: mint_pda,
        freeze_authority: None,
        extensions: Some(vec![ExtensionInstructionData::TokenMetadata(
            TokenMetadataInstructionData {
                update_authority: Some(payer.pubkey().to_bytes().into()),
                name: b"Rent Free Token".to_vec(),
                symbol: b"RFT".to_vec(),
                uri: b"https://example.com/metadata.json".to_vec(),
                additional_metadata: Some(vec![AdditionalMetadata {
                    key: b"type".to_vec(),
                    value: b"compressed".to_vec(),
                }]),
            },
        )]),
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
