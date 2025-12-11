// Test for: transfer-interface.mdx

use borsh::BorshDeserialize;
use light_client::rpc::Rpc;
use light_ctoken_sdk::ctoken::{
    derive_ctoken_ata, CreateAssociatedCTokenAccount, TransferCToken,
    TransferSplToCtoken,
};
use light_ctoken_interface::state::CToken;
use light_program_test::{LightProgramTest, ProgramTestConfig};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use spl_token_2022::state::Account as TokenAccount;
use solana_program::program_pack::Pack;
use anchor_spl;


#[tokio::test(flavor = "multi_thread")]
async fn test_client_transfer_spl_to_ctoken_to_ctoken() {
    // Step 1: Setup test environment
    let mut rpc = LightProgramTest::new(ProgramTestConfig::new_v2(false, None))
        .await
        .unwrap();

    let payer = rpc.get_payer().insecure_clone();
    let sender = Keypair::new();
    airdrop_lamports(&mut rpc, &sender.pubkey(), 1_000_000_000)
        .await
        .unwrap();

    // Step 2: Create SPL mint
    let mint = create_mint_helper(&mut rpc, &payer).await;
    let initial_amount = 10000u64;
    let spl_to_ctoken_amount = 7000u64;
    let ctoken_transfer_amount = 3000u64;

    // Step 3: Create SPL token account and mint tokens
    let spl_token_account = Keypair::new();
    create_token_account(&mut rpc, &mint, &spl_token_account, &sender)
        .await
        .unwrap();

    mint_spl_tokens(
        &mut rpc,
        &mint,
        &spl_token_account.pubkey(),
        &payer,
        initial_amount,
    )
    .await
    .unwrap();

    // Step 4: Create sender cToken ATA
    let (sender_ctoken_ata, _) = derive_ctoken_ata(&sender.pubkey(), &mint);

    let create_sender_ata = CreateAssociatedCTokenAccount::new(
        payer.pubkey(),
        sender.pubkey(),
        mint,
    )
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(&[create_sender_ata], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Step 5: Convert SPL tokens to cToken (7000 tokens)
    let (spl_interface_pda, spl_interface_pda_bump) =
        light_ctoken_sdk::ctoken::get_spl_interface_pda_and_bump(&mint);
    let spl_token_program = anchor_spl::token::ID;

    let spl_to_ctoken_instruction = TransferSplToCtoken {
        amount: spl_to_ctoken_amount,
        spl_interface_pda_bump,
        source_spl_token_account: spl_token_account.pubkey(),
        destination_ctoken_account: sender_ctoken_ata,
        authority: sender.pubkey(),
        mint,
        payer: payer.pubkey(),
        spl_interface_pda,
        spl_token_program,
    }
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(
        &[spl_to_ctoken_instruction],
        &payer.pubkey(),
        &[&payer, &sender],
    )
    .await
    .unwrap();

    // Step 6: Create recipient cToken ATA
    let recipient = Keypair::new();
    airdrop_lamports(&mut rpc, &recipient.pubkey(), 1_000_000_000)
        .await
        .unwrap();

    let (recipient_ctoken_ata, _) = derive_ctoken_ata(&recipient.pubkey(), &mint);

    let create_recipient_ata = CreateAssociatedCTokenAccount::new(
        payer.pubkey(),
        recipient.pubkey(),
        mint,
    )
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(&[create_recipient_ata], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    // Step 7: Transfer cToken from sender to recipient (3000 tokens)
    let transfer_instruction = TransferCToken {
        source: sender_ctoken_ata,
        destination: recipient_ctoken_ata,
        amount: ctoken_transfer_amount,
        authority: sender.pubkey(),
        max_top_up: None,
    }
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(
        &[transfer_instruction],
        &payer.pubkey(),
        &[&payer, &sender],
    )
    .await
    .unwrap();

    // Step 8: Verify balances
    let sender_account_data = rpc.get_account(sender_ctoken_ata).await.unwrap().unwrap();
    let sender_state = CToken::deserialize(&mut &sender_account_data.data[..]).unwrap();

    let recipient_account_data = rpc
        .get_account(recipient_ctoken_ata)
        .await
        .unwrap()
        .unwrap();
    let recipient_state = CToken::deserialize(&mut &recipient_account_data.data[..]).unwrap();

    // Sender should have: 7000 (converted) - 3000 (transferred) = 4000
    assert_eq!(
        sender_state.amount,
        spl_to_ctoken_amount - ctoken_transfer_amount,
        "Sender cToken balance should be 4000"
    );

    // Recipient should have: 3000 (received)
    assert_eq!(
        recipient_state.amount, ctoken_transfer_amount,
        "Recipient cToken balance should be 3000"
    );

    // Verify SPL account still has remaining tokens: 10000 - 7000 = 3000
    let spl_account_data = rpc
        .get_account(spl_token_account.pubkey())
        .await
        .unwrap()
        .unwrap();
    let spl_account = TokenAccount::unpack(&spl_account_data.data).unwrap();
    assert_eq!(
        spl_account.amount,
        initial_amount - spl_to_ctoken_amount,
        "SPL account should have 3000 remaining"
    );
}

// Helper functions inlined from light_test_utils (not available on crates.io)

async fn airdrop_lamports<R: Rpc>(
    rpc: &mut R,
    destination_pubkey: &Pubkey,
    lamports: u64,
) -> Result<(), light_client::rpc::errors::RpcError> {
    let payer = rpc.get_payer().insecure_clone();
    let transfer_ix = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        destination_pubkey,
        lamports,
    );
    rpc.create_and_send_transaction(&[transfer_ix], &payer.pubkey(), &[&payer])
        .await
        .map(|_| ())
}

async fn create_mint_helper<R: Rpc>(rpc: &mut R, payer: &Keypair) -> Pubkey {
    let mint = Keypair::new();
    let rent = rpc
        .get_minimum_balance_for_rent_exemption(anchor_spl::token::Mint::LEN)
        .await
        .unwrap();

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
        2,
    )
    .unwrap();

    rpc.create_and_send_transaction(
        &[create_account_ix, init_mint_ix],
        &payer.pubkey(),
        &[payer, &mint],
    )
    .await
    .unwrap();

    mint.pubkey()
}

async fn create_token_account<R: Rpc>(
    rpc: &mut R,
    mint: &Pubkey,
    account_keypair: &Keypair,
    owner: &Keypair,
) -> Result<(), light_client::rpc::errors::RpcError> {
    let rent = rpc
        .get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)
        .await
        .unwrap();

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
    .unwrap();

    rpc.create_and_send_transaction(
        &[create_account_ix, init_account_ix],
        &owner.pubkey(),
        &[account_keypair, owner],
    )
    .await
    .map(|_| ())
}

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
    .unwrap();

    rpc.create_and_send_transaction(&[mint_to_ix], &mint_authority.pubkey(), &[mint_authority])
        .await
        .map(|_| ())
}
