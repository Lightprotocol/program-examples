use borsh::BorshDeserialize;
use light_client::indexer::{AddressWithTree, Indexer};
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::ctoken::{
    CreateCMint, CreateCMintParams, CreateCTokenAccount, MintToCToken, MintToCTokenParams,
};
use light_ctoken_interface::instructions::extensions::token_metadata::TokenMetadataInstructionData;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use light_ctoken_interface::instructions::extensions::ExtensionInstructionData;
use light_ctoken_interface::instructions::mint_action::CompressedMintWithContext;
use light_ctoken_interface::state::{AdditionalMetadata, CToken, CompressedMint};
use serde_json;
use solana_sdk::{bs58, pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::convert::TryFrom;
use std::env;
use std::fs;

#[tokio::test(flavor = "multi_thread")]
async fn test_mint_to_ctoken() {
    dotenvy::dotenv().ok();

    let keypair_path = env::var("KEYPAIR_PATH")
        .unwrap_or_else(|_| format!("{}/.config/solana/id.json", env::var("HOME").unwrap()));
    let payer = load_keypair(&keypair_path).expect("Failed to load keypair");
    let mint_authority = payer.pubkey();

    let api_key = env::var("api_key")
        .expect("api_key environment variable must be set");

    let config = LightClientConfig::devnet(
        Some("https://devnet.helius-rpc.com".to_string()),
        Some(api_key),
    );
    let mut rpc = LightClient::new_with_retry(config, None)
        .await
        .expect("Failed to initialize LightClient");

    // Step 1: Create compressed mint with metadata
    let (mint, compression_address) = create_compressed_mint(&mut rpc, &payer, 9).await;
    println!("\n=== Created Compressed Mint ===");
    println!("Mint PDA: {}", mint);
    println!(
        "Compression Address: {}",
        bs58::encode(compression_address).into_string()
    );

    // Step 2: Create ctoken account
    let ctoken_account = Keypair::new();
    let owner = payer.pubkey();
    let create_account_ix =
        CreateCTokenAccount::new(payer.pubkey(), ctoken_account.pubkey(), mint, owner)
            .instruction()
            .unwrap();

    rpc.create_and_send_transaction(
        &[create_account_ix],
        &payer.pubkey(),
        &[&payer, &ctoken_account],
    )
    .await
    .unwrap();
    println!("Created ctoken account: {}", ctoken_account.pubkey());

    // Step 3: Get compressed mint account to build CompressedMintWithContext
    let compressed_mint_account = rpc
        .get_compressed_account(compression_address, None)
        .await
        .unwrap()
        .value
        .expect("Compressed mint should exist");

    // Step 4: Get validity proof for the mint operation
    let rpc_result = rpc
        .get_validity_proof(vec![compressed_mint_account.hash], vec![], None)
        .await
        .unwrap()
        .value;

    // Step 5: Deserialize compressed mint data
    let compressed_mint = CompressedMint::deserialize(
        &mut compressed_mint_account.data.unwrap().data.as_slice(),
    )
    .unwrap();

    // Step 6: Build CompressedMintWithContext
    let compressed_mint_with_context = CompressedMintWithContext {
        address: compression_address,
        leaf_index: compressed_mint_account.leaf_index,
        prove_by_index: false,
        root_index: rpc_result.accounts[0]
            .root_index
            .root_index()
            .unwrap_or_default(),
        mint: compressed_mint.try_into().unwrap(),
    };

    let amount = 1_000_000_000u64; // 1 token with 9 decimals

    // Step 7: Get active output queue for devnet
    let _ = rpc.get_latest_active_state_trees().await;
    let output_queue = match rpc
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

    // Step 8: Build mint params
    let params = MintToCTokenParams::new(
        compressed_mint_with_context,
        amount,
        mint_authority,
        rpc_result.proof,
    );

    // Step 9: Build instruction using SDK builder
    let instruction = MintToCToken::new(
        params,
        payer.pubkey(),
        compressed_mint_account.tree_info.tree,
        compressed_mint_account.tree_info.queue,
        output_queue,
        vec![ctoken_account.pubkey()],
    )
    .instruction()
    .unwrap();

    // Step 10: Send transaction
    let compute_unit_ix = ComputeBudgetInstruction::set_compute_unit_limit(300_000);
    rpc.create_and_send_transaction(&[compute_unit_ix, instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();
    println!("Minted {} tokens to ctoken account", amount);

    // Step 11: Verify tokens were minted
    let ctoken_account_data = rpc
        .get_account(ctoken_account.pubkey())
        .await
        .unwrap()
        .unwrap();

    let ctoken_state = CToken::deserialize(&mut &ctoken_account_data.data[..]).unwrap();
    assert_eq!(ctoken_state.amount, amount, "Token amount should match");
    assert_eq!(ctoken_state.mint, mint.to_bytes(), "Mint should match");
    assert_eq!(ctoken_state.owner, owner.to_bytes(), "Owner should match");

    println!("Successfully minted and verified {} tokens!", amount);
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
                name: b"Example Token".to_vec(),
                symbol: b"EXT".to_vec(),
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
