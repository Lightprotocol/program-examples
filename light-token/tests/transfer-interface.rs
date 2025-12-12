use anchor_spl::token::{spl_token, Mint};
use light_client::rpc::{LightClient, LightClientConfig, Rpc};
use light_ctoken_sdk::{
    ctoken::{
        derive_ctoken_ata as derive_token_ata, CreateAssociatedCTokenAccount as CreateAssociatedTokenAccount,
        TransferCToken as TransferToken, TransferSplToCtoken as TransferSplToToken,
    },
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

/// Test SPL → light-token → light-token
//  with ATA creation + transfer in one transaction
#[tokio::test(flavor = "multi_thread")]
async fn test_spl_to_token_to_token() {
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

    // 2. Create SPL mint
    let mint_keypair = Keypair::new();
    let mint = mint_keypair.pubkey();
    let decimals = 2u8;

    let mint_rent = rpc
        .get_minimum_balance_for_rent_exemption(Mint::LEN)
        .await
        .unwrap();

    let create_mint_account_ix = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &mint,
        mint_rent,
        Mint::LEN as u64,
        &spl_token::ID,
    );

    let initialize_mint_ix = spl_token::instruction::initialize_mint(
        &spl_token::ID,
        &mint,
        &payer.pubkey(),
        None,
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

    // 3. Create SPL interface PDA 
    let create_spl_interface_pda_ix =
        CreateSplInterfacePda::new(payer.pubkey(), mint, anchor_spl::token::ID).instruction();

    rpc.create_and_send_transaction(&[create_spl_interface_pda_ix], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    let mint_amount = 10_000u64;
    let spl_to_token_amount = 7_000u64;
    let token_transfer_amount = 3_000u64;

    // 4. Create SPL token account 
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

    // 5. Mint SPL tokens to the SPL account 
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

    // 6. Create sender's token ATA 
    let (sender_token_ata, _bump) = derive_token_ata(&payer.pubkey(), &mint);
    let create_ata_instruction =
        CreateAssociatedTokenAccount::new(payer.pubkey(), payer.pubkey(), mint)
            .instruction()
            .unwrap();

    rpc.create_and_send_transaction(&[create_ata_instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Verify sender's token ATA was created
    let token_account_data = rpc.get_account(sender_token_ata).await.unwrap().unwrap();
    assert!(
        !token_account_data.data.is_empty(),
        "Sender token ATA should exist"
    );

    // 7. Transfer SPL tokens to sender's token account
    let (spl_interface_pda, spl_interface_pda_bump) = find_spl_interface_pda_with_index(&mint, 0);

    let spl_to_token_ix = TransferSplToToken {
        amount: spl_to_token_amount,
        spl_interface_pda_bump,
        source_spl_token_account: spl_token_account_keypair.pubkey(),
        destination_ctoken_account: sender_token_ata,
        authority: payer.pubkey(),
        mint,
        payer: payer.pubkey(),
        spl_interface_pda,
        spl_token_program: anchor_spl::token::ID,
    }
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(&[spl_to_token_ix], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // 8. Create recipient ATA + transfer token→token in one transaction
    let recipient = Keypair::new();
    let (recipient_token_ata, _) = derive_token_ata(&recipient.pubkey(), &mint);

    let create_recipient_ata_ix = CreateAssociatedTokenAccount::new(
        payer.pubkey(),
        recipient.pubkey(),
        mint,
    )
    .instruction()
    .unwrap();

    let token_transfer_ix = TransferToken {
        source: sender_token_ata,
        destination: recipient_token_ata,
        amount: token_transfer_amount,
        authority: payer.pubkey(),
        max_top_up: None,
    }
    .instruction()
    .unwrap();

    let compute_unit_ix = ComputeBudgetInstruction::set_compute_unit_limit(10_000);
    let tx_id = rpc.create_and_send_transaction(
        &[compute_unit_ix, create_recipient_ata_ix, token_transfer_ix],
        &payer.pubkey(),
        &[&payer],
    )
    .await
    .unwrap();
    println!("Sender: {}", payer.pubkey());
    println!("Recipient: {}", recipient.pubkey());
    println!("Amount: {}\n", token_transfer_amount);
    println!(
        "Transaction: https://explorer.solana.com/tx/{}?cluster=devnet\n",
        tx_id
    );
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