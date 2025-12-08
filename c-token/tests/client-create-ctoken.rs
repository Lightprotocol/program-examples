// Test for: client-create-ctoken.mdx


use borsh::BorshDeserialize;
use light_client::rpc::Rpc;
use light_ctoken_sdk::ctoken::CreateCTokenAccount;
use light_ctoken_interface::state::CToken;
use light_program_test::{LightProgramTest, ProgramTestConfig};
use solana_sdk::{signature::Keypair, signer::Signer};
use light_client::indexer::{AddressWithTree, Indexer};
use light_ctoken_sdk::ctoken::{CreateCMint, CreateCMintParams};
use solana_sdk::{pubkey::Pubkey};


#[tokio::test(flavor = "multi_thread")]
async fn test_create_ctoken_account() {
    // Create simulated test environment with Light programs
    let mut rpc = LightProgramTest::new(ProgramTestConfig::new_v2(false, None))
        .await
        .unwrap();

    let payer = rpc.get_payer().insecure_clone();

    // Create compressed mint first (prerequisite)
    let (mint, _compression_address) = create_compressed_mint(&mut rpc, &payer, 9).await;

    // Step 1: Generate new keypair for the cToken account
    let account = Keypair::new();
    let owner = payer.pubkey();

    // Step 2: Build instruction using SDK builder
    let instruction = CreateCTokenAccount::new(
        payer.pubkey(),
        account.pubkey(),
        mint,
        owner,
    )
    .instruction()
    .unwrap();

    // Step 3: Send transaction
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer, &account])
        .await
        .unwrap();

    // Step 4: Verify account creation
    let account_data = rpc.get_account(account.pubkey()).await.unwrap().unwrap();
    let ctoken_state = CToken::deserialize(&mut &account_data.data[..]).unwrap();

    assert_eq!(ctoken_state.mint, mint.to_bytes());
    assert_eq!(ctoken_state.owner, owner.to_bytes());
    assert_eq!(ctoken_state.amount, 0);

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
