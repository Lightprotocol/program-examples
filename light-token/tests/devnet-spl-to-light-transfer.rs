// SPL to light-token to light-token scenario test
//
// This test demonstrates the complete flow:
// 1. Create SPL mint manually
// 2. Create SPL interface PDA (token pool) using SDK instruction
// 3. Create SPL token account
// 4. Mint SPL tokens
// 5. Create sender's cToken ATA (compressible)
// 6. Transfer SPL tokens to cToken account
// 7. Create recipient cATA + transfer cToken→cToken in SAME transaction
// 8. Verify balances

use anchor_spl::token::{spl_token, Mint};
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::{
    ctoken::{derive_ctoken_ata, CreateAssociatedCTokenAccount, TransferCToken, TransferSplToCtoken},
    spl_interface::{find_spl_interface_pda_with_index, CreateSplInterfacePda},
};
use serde_json;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::program_pack::Pack;
use solana_sdk::{signature::Keypair, signer::Signer};
use spl_token_2022::pod::PodAccount;
use std::convert::TryFrom;
use std::env;
use std::fs;

/// Test SPL → cToken → cToken flow with combined ATA creation + transfer
#[tokio::test(flavor = "multi_thread")]
async fn test_spl_to_ctoken_to_ctoken() {
    // 1. Setup test environment - load from .env
    dotenvy::dotenv().ok();

    let keypair_path = env::var("KEYPAIR_PATH")
        .unwrap_or_else(|_| format!("{}/.config/solana/id.json", env::var("HOME").unwrap()));
    let payer = load_keypair(&keypair_path).expect("Failed to load keypair");
    let api_key = env::var("api_key")
        .expect("api_key environment variable must be set. Create a .env file or set it in your environment.");

    let config = LightClientConfig::devnet(
        Some("https://devnet.helius-rpc.com".to_string()),
        Some(api_key),
    );
    let mut rpc = LightClient::new_with_retry(config, None)
        .await
        .expect("Failed to initialize LightClient");

    // 2. Create SPL mint manually
    let mint_keypair = Keypair::new();
    let mint = mint_keypair.pubkey();
    let decimals = 2u8;

    // Get rent for mint account
    let mint_rent = rpc
        .get_minimum_balance_for_rent_exemption(Mint::LEN)
        .await
        .unwrap();

    // Create mint account instruction
    let create_mint_account_ix = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &mint,
        mint_rent,
        Mint::LEN as u64,
        &spl_token::ID,
    );

    // Initialize mint instruction
    let initialize_mint_ix = spl_token::instruction::initialize_mint(
        &spl_token::ID,
        &mint,
        &payer.pubkey(), // mint authority
        None,            // freeze authority
        decimals,
    )
    .unwrap();

    rpc.create_and_send_transaction(
        &[create_mint_account_ix, initialize_mint_ix],
        &payer.pubkey(),
        &[&payer, &mint_keypair],
    )
    .await
    .unwrap();

    // 3. Create SPL interface PDA (token pool) using SDK instruction
    let create_spl_interface_pda_ix =
        CreateSplInterfacePda::new(payer.pubkey(), mint, anchor_spl::token::ID).instruction();

    rpc.create_and_send_transaction(&[create_spl_interface_pda_ix], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    let mint_amount = 10_000u64;
    let spl_to_ctoken_amount = 7_000u64;
    let ctoken_transfer_amount = 3_000u64;

    // 4. Create SPL token account (inline)
    let spl_token_account_keypair = Keypair::new();
    let token_account_rent = rpc
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)
        .await
        .unwrap();
    let create_token_account_ix = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &spl_token_account_keypair.pubkey(),
        token_account_rent,
        spl_token::state::Account::LEN as u64,
        &spl_token::ID,
    );
    let init_token_account_ix = spl_token::instruction::initialize_account(
        &spl_token::ID,
        &spl_token_account_keypair.pubkey(),
        &mint,
        &payer.pubkey(),
    )
    .unwrap();
    rpc.create_and_send_transaction(
        &[create_token_account_ix, init_token_account_ix],
        &payer.pubkey(),
        &[&spl_token_account_keypair, &payer],
    )
    .await
    .unwrap();

    // 5. Mint SPL tokens to the SPL account (inline)
    let mint_to_ix = spl_token::instruction::mint_to(
        &spl_token::ID,
        &mint,
        &spl_token_account_keypair.pubkey(),
        &payer.pubkey(),
        &[&payer.pubkey()],
        mint_amount,
    )
    .unwrap();
    rpc.create_and_send_transaction(&[mint_to_ix], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Verify SPL account has tokens
    let spl_account_data = rpc
        .get_account(spl_token_account_keypair.pubkey())
        .await
        .unwrap()
        .unwrap();
    let spl_account =
        spl_pod::bytemuck::pod_from_bytes::<PodAccount>(&spl_account_data.data).unwrap();
    let initial_spl_balance: u64 = spl_account.amount.into();
    assert_eq!(initial_spl_balance, mint_amount);

    // 6. Create sender's cToken ATA (compressible with default 16 prepaid epochs)
    let (sender_ctoken_ata, _bump) = derive_ctoken_ata(&payer.pubkey(), &mint);
    let create_ata_instruction =
        CreateAssociatedCTokenAccount::new(payer.pubkey(), payer.pubkey(), mint)
            .instruction()
            .unwrap();

    rpc.create_and_send_transaction(&[create_ata_instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Verify sender's cToken ATA was created
    let ctoken_account_data = rpc.get_account(sender_ctoken_ata).await.unwrap().unwrap();
    assert!(
        !ctoken_account_data.data.is_empty(),
        "Sender cToken ATA should exist"
    );

    // 7. Transfer SPL tokens to sender's cToken account
    let (spl_interface_pda, spl_interface_pda_bump) = find_spl_interface_pda_with_index(&mint, 0);

    let spl_to_ctoken_ix = TransferSplToCtoken {
        amount: spl_to_ctoken_amount,
        spl_interface_pda_bump,
        source_spl_token_account: spl_token_account_keypair.pubkey(),
        destination_ctoken_account: sender_ctoken_ata,
        authority: payer.pubkey(),
        mint,
        payer: payer.pubkey(),
        spl_interface_pda,
        spl_token_program: anchor_spl::token::ID,
    }
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(&[spl_to_ctoken_ix], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // 8. Create recipient cATA + transfer cToken→cToken in SAME transaction
    let recipient = Keypair::new();
    let (recipient_ctoken_ata, _) = derive_ctoken_ata(&recipient.pubkey(), &mint);

    let create_recipient_ata_ix = CreateAssociatedCTokenAccount::new(
        payer.pubkey(),
        recipient.pubkey(),
        mint,
    )
    .instruction()
    .unwrap();

    let ctoken_transfer_ix = TransferCToken {
        source: sender_ctoken_ata,
        destination: recipient_ctoken_ata,
        amount: ctoken_transfer_amount,
        authority: payer.pubkey(),
        max_top_up: None,
    }
    .instruction()
    .unwrap();

    // COMBINED: create recipient ATA + transfer in one transaction
    let compute_unit_ix = ComputeBudgetInstruction::set_compute_unit_limit(10_000);
    let tx_id = rpc.create_and_send_transaction(
        &[compute_unit_ix, create_recipient_ata_ix, ctoken_transfer_ix],
        &payer.pubkey(),
        &[&payer],
    )
    .await
    .unwrap();
    println!("tx_id: {}", tx_id);

    // 9. Verify results
    // Check SPL account balance decreased
    let spl_account_data = rpc
        .get_account(spl_token_account_keypair.pubkey())
        .await
        .unwrap()
        .unwrap();
    let spl_account =
        spl_pod::bytemuck::pod_from_bytes::<PodAccount>(&spl_account_data.data).unwrap();
    let final_spl_balance: u64 = spl_account.amount.into();
    assert_eq!(
        final_spl_balance,
        mint_amount - spl_to_ctoken_amount,
        "SPL account balance should be 3000"
    );

    // Check sender cToken balance (7000 - 3000 = 4000)
    let sender_ctoken_data = rpc.get_account(sender_ctoken_ata).await.unwrap().unwrap();
    let sender_ctoken =
        spl_pod::bytemuck::pod_from_bytes::<PodAccount>(&sender_ctoken_data.data[..165]).unwrap();
    let sender_ctoken_balance: u64 = sender_ctoken.amount.into();
    assert_eq!(
        sender_ctoken_balance,
        spl_to_ctoken_amount - ctoken_transfer_amount,
        "Sender cToken balance should be 4000"
    );

    // Check recipient cToken balance (3000)
    let recipient_ctoken_data = rpc.get_account(recipient_ctoken_ata).await.unwrap().unwrap();
    let recipient_ctoken =
        spl_pod::bytemuck::pod_from_bytes::<PodAccount>(&recipient_ctoken_data.data[..165]).unwrap();
    let recipient_ctoken_balance: u64 = recipient_ctoken.amount.into();
    assert_eq!(
        recipient_ctoken_balance, ctoken_transfer_amount,
        "Recipient cToken balance should be 3000"
    );

    println!("SPL → cToken → cToken transfer completed!");
    println!("  - Created SPL mint: {}", mint);
    println!("  - Minted {} tokens to SPL account", mint_amount);
    println!("  - Transferred {} SPL → sender cToken", spl_to_ctoken_amount);
    println!(
        "  - Transferred {} cToken → recipient cToken (in same tx as ATA creation)",
        ctoken_transfer_amount
    );
    println!("\nFinal balances:");
    println!("  - SPL account: {}", final_spl_balance);
    println!("  - Sender cToken: {}", sender_ctoken_balance);
    println!("  - Recipient cToken: {}", recipient_ctoken_balance);

    println!("\nTest passed!");
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