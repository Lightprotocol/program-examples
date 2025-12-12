use borsh::BorshDeserialize;
use light_client::indexer::{AddressWithTree, Indexer};
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::ctoken::{
    derive_ctoken_ata, CreateAssociatedCTokenAccount, CreateCMint, CreateCMintParams,
};
use light_ctoken_interface::state::CToken;
use serde_json;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::convert::TryFrom;
use std::env;
use std::fs;

#[tokio::test(flavor = "multi_thread")]
async fn test_create_associated_token_account() {
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

    // Step 2: Define owner and derive ATA address
    let owner = payer.pubkey();
    let (ata_address, _bump) = derive_ctoken_ata(&owner, &mint);

    // Step 3: Build instruction using SDK builder
    let instruction = CreateAssociatedCTokenAccount::new(payer.pubkey(), owner, mint)
        .instruction()
        .unwrap();

    // Step 4: Send transaction (only payer signs, no account keypair needed)
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Step 5: Verify cATA creation
    let account_data = rpc.get_account(ata_address).await.unwrap().unwrap();
    let ctoken_state = CToken::deserialize(&mut &account_data.data[..]).unwrap();

    assert_eq!(ctoken_state.mint, mint.to_bytes(), "Mint should match");
    assert_eq!(ctoken_state.owner, owner.to_bytes(), "Owner should match");
    assert_eq!(ctoken_state.amount, 0, "Initial amount should be 0");
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

    // Build params (no metadata for this simple example)
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
