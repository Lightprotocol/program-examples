// Devnet example: SPL token → cToken → cToken transfer
//
// This example demonstrates the full flow on devnet:
// 1. Create SPL mint
// 2. Mint SPL tokens
// 3. Transfer SPL tokens to cToken
// 4. Transfer cToken to another cToken account
//
// Run with:
//   cargo test devnet_spl_to_ctoken --release -- --ignored --nocapture
//
// Requires:
//   - KEYPAIR_PATH env var (default: ~/.config/solana/id.json)
//   - PHOTON_URL env var (default: https://photon-devnet.helius.dev)
//   - Devnet SOL in the keypair

use borsh::BorshDeserialize;
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::ctoken::{
    derive_ctoken_ata, CreateAssociatedCTokenAccount, TransferCToken, TransferSplToCtoken,
};
use light_ctoken_interface::state::CToken;
use solana_program::program_pack::Pack;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use spl_token_2022::state::Account as TokenAccount;
use std::env;

#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires devnet SOL and indexer - run with --ignored flag
async fn test_devnet_spl_to_ctoken_to_ctoken() {
    // Step 1: Setup devnet connection
    let keypair_path = env::var("KEYPAIR_PATH")
        .unwrap_or_else(|_| format!("{}/.config/solana/id.json", env::var("HOME").unwrap()));

    let payer = load_keypair(&keypair_path).expect("Failed to load keypair");
    println!("Using payer: {}", payer.pubkey());

    let photon_url = env::var("PHOTON_URL")
        .unwrap_or_else(|_| "https://photon-devnet.helius.dev".to_string());

    let config = LightClientConfig::devnet(Some(photon_url), None);
    let mut rpc = LightClient::new_with_retry(config, None)
        .await
        .expect("Failed to connect to devnet");

    println!("Connected to devnet");

    // Step 2: Create SPL mint
    let mint = create_mint_helper(&mut rpc, &payer).await;
    println!("Created SPL mint: {}", mint);

    let initial_amount = 10000u64;
    let spl_to_ctoken_amount = 7000u64;
    let ctoken_transfer_amount = 3000u64;

    // Step 3: Create SPL token account and mint tokens
    let spl_token_account = Keypair::new();
    create_token_account(&mut rpc, &mint, &spl_token_account, &payer)
        .await
        .expect("Failed to create token account");
    println!("Created SPL token account: {}", spl_token_account.pubkey());

    mint_spl_tokens(
        &mut rpc,
        &mint,
        &spl_token_account.pubkey(),
        &payer,
        initial_amount,
    )
    .await
    .expect("Failed to mint SPL tokens");
    println!("Minted {} SPL tokens", initial_amount);

    // Step 4: Create sender cToken ATA
    let sender = Keypair::new();
    let (sender_ctoken_ata, _) = derive_ctoken_ata(&sender.pubkey(), &mint);

    let create_sender_ata = CreateAssociatedCTokenAccount::new(
        payer.pubkey(),
        sender.pubkey(),
        mint,
    )
    .instruction()
    .expect("Failed to create sender ATA instruction");

    rpc.create_and_send_transaction(&[create_sender_ata], &payer.pubkey(), &[&payer])
        .await
        .expect("Failed to create sender cToken ATA");
    println!("Created sender cToken ATA: {}", sender_ctoken_ata);

    // Step 5: Convert SPL tokens to cToken (7000 tokens)
    let (spl_interface_pda, spl_interface_pda_bump) =
        light_ctoken_sdk::ctoken::get_spl_interface_pda_and_bump(&mint);
    let spl_token_program = anchor_spl::token::ID;

    let spl_to_ctoken_instruction = TransferSplToCtoken {
        amount: spl_to_ctoken_amount,
        spl_interface_pda_bump,
        source_spl_token_account: spl_token_account.pubkey(),
        destination_ctoken_account: sender_ctoken_ata,
        authority: payer.pubkey(), // payer owns the SPL token account
        mint,
        payer: payer.pubkey(),
        spl_interface_pda,
        spl_token_program,
    }
    .instruction()
    .expect("Failed to create SPL to cToken instruction");

    rpc.create_and_send_transaction(
        &[spl_to_ctoken_instruction],
        &payer.pubkey(),
        &[&payer],
    )
    .await
    .expect("Failed to transfer SPL to cToken");
    println!(
        "Transferred {} SPL tokens to cToken",
        spl_to_ctoken_amount
    );

    // Step 6: Create recipient cToken ATA
    let recipient = Keypair::new();
    let (recipient_ctoken_ata, _) = derive_ctoken_ata(&recipient.pubkey(), &mint);

    let create_recipient_ata = CreateAssociatedCTokenAccount::new(
        payer.pubkey(),
        recipient.pubkey(),
        mint,
    )
    .instruction()
    .expect("Failed to create recipient ATA instruction");

    rpc.create_and_send_transaction(&[create_recipient_ata], &payer.pubkey(), &[&payer])
        .await
        .expect("Failed to create recipient cToken ATA");
    println!("Created recipient cToken ATA: {}", recipient_ctoken_ata);

    // Step 7: Transfer cToken from sender to recipient (3000 tokens)
    let transfer_instruction = TransferCToken {
        source: sender_ctoken_ata,
        destination: recipient_ctoken_ata,
        amount: ctoken_transfer_amount,
        authority: sender.pubkey(),
        max_top_up: None,
    }
    .instruction()
    .expect("Failed to create cToken transfer instruction");

    rpc.create_and_send_transaction(
        &[transfer_instruction],
        &payer.pubkey(),
        &[&payer, &sender],
    )
    .await
    .expect("Failed to transfer cToken");
    println!(
        "Transferred {} cTokens from sender to recipient",
        ctoken_transfer_amount
    );

    // Step 8: Verify balances
    let sender_account_data = rpc
        .get_account(sender_ctoken_ata)
        .await
        .expect("Failed to get sender account")
        .expect("Sender account not found");
    let sender_state = CToken::deserialize(&mut &sender_account_data.data[..])
        .expect("Failed to deserialize sender cToken");

    let recipient_account_data = rpc
        .get_account(recipient_ctoken_ata)
        .await
        .expect("Failed to get recipient account")
        .expect("Recipient account not found");
    let recipient_state = CToken::deserialize(&mut &recipient_account_data.data[..])
        .expect("Failed to deserialize recipient cToken");

    // Sender should have: 7000 (converted) - 3000 (transferred) = 4000
    assert_eq!(
        sender_state.amount,
        spl_to_ctoken_amount - ctoken_transfer_amount,
        "Sender cToken balance should be 4000"
    );
    println!(
        "Sender cToken balance: {} (expected: {})",
        sender_state.amount,
        spl_to_ctoken_amount - ctoken_transfer_amount
    );

    // Recipient should have: 3000 (received)
    assert_eq!(
        recipient_state.amount, ctoken_transfer_amount,
        "Recipient cToken balance should be 3000"
    );
    println!(
        "Recipient cToken balance: {} (expected: {})",
        recipient_state.amount, ctoken_transfer_amount
    );

    // Verify SPL account still has remaining tokens: 10000 - 7000 = 3000
    let spl_account_data = rpc
        .get_account(spl_token_account.pubkey())
        .await
        .expect("Failed to get SPL account")
        .expect("SPL account not found");
    let spl_account = TokenAccount::unpack(&spl_account_data.data)
        .expect("Failed to unpack SPL account");
    assert_eq!(
        spl_account.amount,
        initial_amount - spl_to_ctoken_amount,
        "SPL account should have 3000 remaining"
    );
    println!(
        "SPL account balance: {} (expected: {})",
        spl_account.amount,
        initial_amount - spl_to_ctoken_amount
    );

    println!("\nAll assertions passed!");
}

// Helper: Load keypair from file
fn load_keypair(path: &str) -> Result<Keypair, Box<dyn std::error::Error>> {
    let path = if path.starts_with("~") {
        path.replace("~", &env::var("HOME").unwrap_or_default())
    } else {
        path.to_string()
    };
    let file = std::fs::read_to_string(&path)?;
    let bytes: Vec<u8> = serde_json::from_str(&file)?;
    Ok(Keypair::from_bytes(&bytes)?)
}

// Helper: Create SPL mint
async fn create_mint_helper<R: Rpc>(rpc: &mut R, payer: &Keypair) -> Pubkey {
    let mint = Keypair::new();
    let rent = rpc
        .get_minimum_balance_for_rent_exemption(anchor_spl::token::Mint::LEN)
        .await
        .expect("Failed to get rent");

    let create_account_ix = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        anchor_spl::token::Mint::LEN as u64,
        &spl_token::ID,
    );

    let init_mint_ix = spl_token::instruction::initialize_mint(
        &spl_token::ID,
        &mint.pubkey(),
        &payer.pubkey(),
        Some(&payer.pubkey()),
        2, // decimals
    )
    .expect("Failed to create init mint instruction");

    rpc.create_and_send_transaction(
        &[create_account_ix, init_mint_ix],
        &payer.pubkey(),
        &[payer, &mint],
    )
    .await
    .expect("Failed to create mint");

    mint.pubkey()
}

// Helper: Create SPL token account
async fn create_token_account<R: Rpc>(
    rpc: &mut R,
    mint: &Pubkey,
    account_keypair: &Keypair,
    owner: &Keypair,
) -> Result<(), light_client::rpc::errors::RpcError> {
    let rent = rpc
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)
        .await?;

    let create_account_ix = solana_sdk::system_instruction::create_account(
        &owner.pubkey(),
        &account_keypair.pubkey(),
        rent,
        spl_token::state::Account::LEN as u64,
        &spl_token::ID,
    );

    let init_account_ix = spl_token::instruction::initialize_account(
        &spl_token::ID,
        &account_keypair.pubkey(),
        mint,
        &owner.pubkey(),
    )
    .expect("Failed to create init account instruction");

    rpc.create_and_send_transaction(
        &[create_account_ix, init_account_ix],
        &owner.pubkey(),
        &[account_keypair, owner],
    )
    .await
    .map(|_| ())
}

// Helper: Mint SPL tokens
async fn mint_spl_tokens<R: Rpc>(
    rpc: &mut R,
    mint: &Pubkey,
    token_account: &Pubkey,
    mint_authority: &Keypair,
    amount: u64,
) -> Result<(), light_client::rpc::errors::RpcError> {
    let mint_to_ix = spl_token::instruction::mint_to(
        &spl_token::ID,
        mint,
        token_account,
        &mint_authority.pubkey(),
        &[&mint_authority.pubkey()],
        amount,
    )
    .expect("Failed to create mint_to instruction");

    rpc.create_and_send_transaction(&[mint_to_ix], &mint_authority.pubkey(), &[mint_authority])
        .await
        .map(|_| ())
}
