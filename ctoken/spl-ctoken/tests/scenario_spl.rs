// SPL to cToken scenario test - Direct SDK calls without wrapper program
//
// This test demonstrates the complete flow:
// 1. Create SPL mint manually
// 2. Create SPL interface PDA (token pool) using SDK instruction
// 3. Create SPL token account
// 4. Mint SPL tokens
// 5. Create cToken ATA (compressible)
// 6. Transfer SPL tokens to cToken account
// 7. Verify balances

use anchor_spl::token::{spl_token, Mint};
use light_client::rpc::Rpc;
use light_ctoken_sdk::{
    ctoken::{derive_ctoken_ata, CreateAssociatedCTokenAccount, TransferSplToCtoken},
    spl_interface::{find_spl_interface_pda_with_index, CreateSplInterfacePda},
};
use light_program_test::{LightProgramTest, ProgramTestConfig};
use solana_sdk::program_pack::Pack;
use solana_sdk::{signature::Keypair, signer::Signer};
use spl_token_2022::pod::PodAccount;

/// Test the complete SPL to cToken flow using direct SDK calls
#[tokio::test]
async fn test_spl_to_ctoken_scenario() {
    // 1. Setup test environment
    let mut rpc = LightProgramTest::new(ProgramTestConfig::new_v2(false, None))
        .await
        .unwrap();

    let payer = rpc.get_payer().insecure_clone();

    // Create a token owner
    let token_owner = Keypair::new();
    rpc.airdrop_lamports(&token_owner.pubkey(), 1_000_000_000)
        .await
        .unwrap();

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
    let transfer_amount = 5_000u64;

    // 4. Create SPL token account (inline)
    let spl_token_account_keypair = Keypair::new();
    let token_account_rent = rpc
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)
        .await
        .unwrap();
    let create_token_account_ix = solana_sdk::system_instruction::create_account(
        &token_owner.pubkey(),
        &spl_token_account_keypair.pubkey(),
        token_account_rent,
        spl_token::state::Account::LEN as u64,
        &spl_token::ID,
    );
    let init_token_account_ix = spl_token::instruction::initialize_account(
        &spl_token::ID,
        &spl_token_account_keypair.pubkey(),
        &mint,
        &token_owner.pubkey(),
    )
    .unwrap();
    rpc.create_and_send_transaction(
        &[create_token_account_ix, init_token_account_ix],
        &token_owner.pubkey(),
        &[&spl_token_account_keypair, &token_owner],
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

    // 6. Create cToken ATA for the recipient (compressible with default 16 prepaid epochs)
    let ctoken_recipient = Keypair::new();
    rpc.airdrop_lamports(&ctoken_recipient.pubkey(), 1_000_000_000)
        .await
        .unwrap();

    let (ctoken_ata, _bump) = derive_ctoken_ata(&ctoken_recipient.pubkey(), &mint);
    let create_ata_instruction =
        CreateAssociatedCTokenAccount::new(payer.pubkey(), ctoken_recipient.pubkey(), mint)
            .instruction()
            .unwrap();

    rpc.create_and_send_transaction(&[create_ata_instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Verify cToken ATA was created
    let ctoken_account_data = rpc.get_account(ctoken_ata).await.unwrap().unwrap();
    assert!(
        !ctoken_account_data.data.is_empty(),
        "cToken ATA should exist"
    );

    // 7. Transfer SPL tokens to cToken account
    let (spl_interface_pda, spl_interface_pda_bump) = find_spl_interface_pda_with_index(&mint, 0);

    let transfer_instruction = TransferSplToCtoken {
        amount: transfer_amount,
        spl_interface_pda_bump,
        source_spl_token_account: spl_token_account_keypair.pubkey(),
        destination_ctoken_account: ctoken_ata,
        authority: token_owner.pubkey(),
        mint,
        payer: payer.pubkey(),
        spl_interface_pda,
        spl_token_program: anchor_spl::token::ID,
    }
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(
        &[transfer_instruction],
        &payer.pubkey(),
        &[&payer, &token_owner],
    )
    .await
    .unwrap();

    // 7. Verify results
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
        mint_amount - transfer_amount,
        "SPL account balance should have decreased by transfer amount"
    );

    // Check cToken account balance increased
    let ctoken_account_data = rpc.get_account(ctoken_ata).await.unwrap().unwrap();
    let ctoken_account =
        spl_pod::bytemuck::pod_from_bytes::<PodAccount>(&ctoken_account_data.data[..165]).unwrap();
    let ctoken_balance: u64 = ctoken_account.amount.into();
    assert_eq!(
        ctoken_balance, transfer_amount,
        "cToken account should have received the transferred tokens"
    );

    println!("SPL to cToken transfer completed!");
    println!("  - Created SPL mint: {}", mint);
    println!(
        "  - Created SPL token account: {}",
        spl_token_account_keypair.pubkey()
    );
    println!("  - Minted {} tokens to SPL account", mint_amount);
    println!("  - Created cToken ATA: {}", ctoken_ata);
    println!(
        "  - Transferred {} tokens from SPL to cToken",
        transfer_amount
    );
    println!(
        "  - Final SPL balance: {}, cToken balance: {}",
        final_spl_balance, ctoken_balance
    );

    println!("\nSPL to cToken transfer test passed!");
}
