// Test for: client-create-cmint.mdx

use light_client::indexer::{AddressWithTree, Indexer};
use light_client::rpc::Rpc;
use light_ctoken_sdk::ctoken::{CreateCMint, CreateCMintParams};
use light_ctoken_interface::instructions::extensions::token_metadata::TokenMetadataInstructionData;
use light_ctoken_interface::instructions::extensions::ExtensionInstructionData;
use light_ctoken_interface::state::AdditionalMetadata;
use light_program_test::{LightProgramTest, ProgramTestConfig};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};


#[tokio::test(flavor = "multi_thread")]
async fn test_create_rent_free_mint_with_metadata() {
    let mut rpc = LightProgramTest::new(ProgramTestConfig::new_v2(false, None))
        .await
        .unwrap();

    let payer = rpc.get_payer().insecure_clone();

    // Create c-mint with metadata
    let (_mint, _compression_address) = create_compressed_mint(&mut rpc, &payer, 9).await;

}
pub async fn create_compressed_mint<R: Rpc + Indexer>(
    rpc: &mut R,
    payer: &Keypair,
    decimals: u8,
) -> (Pubkey, [u8; 32]) {
    let mint_signer = Keypair::new();
    let address_tree = rpc.get_address_tree_v2();
    let output_queue = rpc.get_random_state_tree_info().unwrap().queue;

    // Derive address
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
                name: b"Rent Free Token".to_vec(),
                symbol: b"RFT".to_vec(),
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
        output_queue,
    );
    let instruction = create_cmint.instruction().unwrap();

    // Send transaction
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer, &mint_signer])
        .await
        .unwrap();

    (mint_pda, compression_address)
}
