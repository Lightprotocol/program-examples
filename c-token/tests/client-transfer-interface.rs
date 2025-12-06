// Test for: transfer-interface.mdx

use borsh::BorshDeserialize;
use light_client::rpc::Rpc;
use light_ctoken_sdk::{
    ctoken::{
        derive_ctoken_ata, CreateAssociatedTokenAccount, TransferCtoken,
        TransferSplToCtoken,
    },
    token_pool::find_token_pool_pda_with_index,
};
use light_ctoken_interface::state::CToken;
use light_program_test::{LightProgramTest, ProgramTestConfig};
use light_test_utils::spl::{create_mint_helper, create_token_2022_account, mint_spl_tokens};
use solana_sdk::{signature::Keypair, signer::Signer};
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
    light_test_utils::airdrop_lamports(&mut rpc, &sender.pubkey(), 1_000_000_000)
        .await
        .unwrap();

    // Step 2: Create SPL mint
    let mint = create_mint_helper(&mut rpc, &payer).await;
    let initial_amount = 10000u64;
    let spl_to_ctoken_amount = 7000u64;
    let ctoken_transfer_amount = 3000u64;

    // Step 3: Create SPL token account and mint tokens
    let spl_token_account = Keypair::new();
    create_token_2022_account(&mut rpc, &mint, &spl_token_account, &sender, false)
        .await
        .unwrap();

    mint_spl_tokens(
        &mut rpc,
        &mint,
        &spl_token_account.pubkey(),
        &payer.pubkey(),
        &payer,
        initial_amount,
        false,
    )
    .await
    .unwrap();

    // Step 4: Create sender cToken ATA
    let (sender_ctoken_ata, _) = derive_ctoken_ata(&sender.pubkey(), &mint);

    let create_sender_ata = CreateAssociatedTokenAccount::new(
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
    let (token_pool_pda, token_pool_pda_bump) = find_token_pool_pda_with_index(&mint, 0);
    let spl_token_program = anchor_spl::token::ID;

    let spl_to_ctoken_instruction = TransferSplToCtoken {
        amount: spl_to_ctoken_amount,
        token_pool_pda_bump,
        source_spl_token_account: spl_token_account.pubkey(),
        destination_ctoken_account: sender_ctoken_ata,
        authority: sender.pubkey(),
        mint,
        payer: payer.pubkey(),
        token_pool_pda,
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
    light_test_utils::airdrop_lamports(&mut rpc, &recipient.pubkey(), 1_000_000_000)
        .await
        .unwrap();

    let (recipient_ctoken_ata, _) = derive_ctoken_ata(&recipient.pubkey(), &mint);

    let create_recipient_ata = CreateAssociatedTokenAccount::new(
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
    let transfer_instruction = TransferCtoken {
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
