// Test for: client-mint-to-ctoken.mdx

use borsh::BorshDeserialize;
use light_client::indexer::{AddressWithTree, Indexer};
use light_client::rpc::Rpc;
use light_ctoken_sdk::ctoken::{
    CreateCMint, CreateCMintParams, CreateCTokenAccount, MintToCToken,
    MintToCTokenParams,
};
use light_ctoken_interface::instructions::mint_action::CompressedMintWithContext;
use light_ctoken_interface::state::{CToken, CompressedMint};
use light_program_test::{LightProgramTest, ProgramTestConfig};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};


#[tokio::test(flavor = "multi_thread")]
async fn test_mint_to_ctoken_client() {
    // Step 1: Setup test environment
    let mut rpc = LightProgramTest::new(ProgramTestConfig::new_v2(false, None))
        .await
        .unwrap();

    let payer = rpc.get_payer().insecure_clone();
    let mint_authority = payer.pubkey();

    // Step 2: Create compressed mint (prerequisite)
    let (mint, compression_address) = create_compressed_mint(&mut rpc, &payer, 9).await;

    // Step 3: Create ctoken account (prerequisite)
    let ctoken_account = Keypair::new();
    let owner = payer.pubkey();
    let create_account_ix = CreateCTokenAccount::new(
        payer.pubkey(),
        ctoken_account.pubkey(),
        mint,
        owner,
    )
    .instruction()
    .unwrap();

    rpc.create_and_send_transaction(
        &[create_account_ix],
        &payer.pubkey(),
        &[&payer, &ctoken_account],
    )
    .await
    .unwrap();

    // Step 4: Get compressed mint account to build CompressedMintWithContext
    let compressed_mint_account = rpc
        .get_compressed_account(compression_address, None)
        .await
        .unwrap()
        .value
        .expect("Compressed mint should exist");

    // Step 5: Get validity proof for the mint operation
    let rpc_result = rpc
        .get_validity_proof(vec![compressed_mint_account.hash], vec![], None)
        .await
        .unwrap()
        .value;

    // Step 6: Deserialize compressed mint data
    let compressed_mint = CompressedMint::deserialize(
        &mut compressed_mint_account.data.unwrap().data.as_slice(),
    )
    .unwrap();

    // Step 7: Build CompressedMintWithContext
    let compressed_mint_with_context = CompressedMintWithContext {
        address: compression_address,
        leaf_index: compressed_mint_account.leaf_index,
        prove_by_index: true,
        root_index: rpc_result.accounts[0]
            .root_index
            .root_index()
            .unwrap_or_default(),
        mint: compressed_mint.try_into().unwrap(),
    };

    let amount = 1_000_000_000u64; // 1 token with 9 decimals

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
        compressed_mint_account.tree_info.queue,
        vec![ctoken_account.pubkey()],
    )
    .instruction()
    .unwrap();

    // Step 10: Send transaction
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

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
}
pub async fn create_compressed_mint<R: Rpc + Indexer>(
    rpc: &mut R,
    payer: &Keypair,
    decimals: u8,
) -> (Pubkey, [u8; 32]) {
    let mint_signer = Keypair::new();
    let address_tree = rpc.get_address_tree_v2();
    let output_queue = rpc.get_random_state_tree_info().unwrap().queue;

    // Derive compression address
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
        output_queue,
    );
    let instruction = create_cmint.instruction().unwrap();

    // Send transaction
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer, &mint_signer])
        .await
        .unwrap();

    (mint_pda, compression_address)
}
