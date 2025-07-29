use anchor_lang::prelude::borsh::BorshDeserialize;
use anchor_lang::{InstructionData, ToAccountMetas};
use ctoken_minter::{
    instructions::{CreateCompressedMintInstructionData, MintCompressedTokensInstructionData},
    ID as CTOKEN_MINTER_ID,
};
use light_client::indexer::Indexer;
use light_compressed_token_sdk::instructions::create_compressed_mint::find_spl_mint_address;
use light_compressed_token_sdk::instructions::{
    derive_compressed_mint_address, get_create_compressed_mint_instruction_account_metas,
    get_mint_to_compressed_instruction_account_metas, CreateCompressedMintMetaConfig,
    MintToCompressedMetaConfig,
};
use light_ctoken_types::{
    instructions::{
        extensions::token_metadata::TokenMetadataInstructionData,
        mint_to_compressed::{CompressedMintInputs, Recipient},
    },
    state::{
        extensions::{AdditionalMetadata, Metadata},
        CompressedMint,
    },
};
use light_program_test::{LightProgramTest, ProgramTestConfig, Rpc, RpcError};

use solana_pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature, Signer};

#[tokio::test]
async fn test_ctoken_minter() {
    // Initialize test environment
    let config = ProgramTestConfig::new_v2(false, Some(vec![("ctoken_minter", CTOKEN_MINTER_ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // Test parameters
    let decimals = 6u8;
    let mint_authority_keypair = Keypair::new();
    let mint_authority = mint_authority_keypair.pubkey();
    let freeze_authority = mint_authority; // Same as mint authority for this example
    let mint_seed = Keypair::new();

    // Token metadata
    let token_name = "Test Compressed Token".to_string();
    let token_symbol = "TCT".to_string();
    let token_uri = "https://example.com/test-token.json".to_string();

    // Create token metadata extension
    let additional_metadata = vec![
        AdditionalMetadata {
            key: b"created_by".to_vec(),
            value: b"ctoken-minter".to_vec(),
        },
        AdditionalMetadata {
            key: b"example".to_vec(),
            value: b"program-examples".to_vec(),
        },
    ];

    let token_metadata = TokenMetadataInstructionData {
        update_authority: Some(mint_authority.into()),
        metadata: Metadata {
            name: token_name.clone().into_bytes(),
            symbol: token_symbol.clone().into_bytes(),
            uri: token_uri.clone().into_bytes(),
        },
        additional_metadata: Some(additional_metadata),
        version: 0, // Poseidon hash version
    };

    // Create the compressed mint
    let compressed_mint_address = create_mint(
        &mut rpc,
        &mint_seed,
        decimals,
        &mint_authority_keypair,
        Some(freeze_authority),
        Some(token_metadata),
        &payer,
    )
    .await
    .unwrap();

    // Assert the compressed mint was created
    let compressed_mint_account = rpc
        .indexer()
        .unwrap()
        .get_compressed_account(compressed_mint_address, None)
        .await
        .expect("Failed to get compressed mint account")
        .value;
    
    assert!(compressed_mint_account.data.is_some(), "Compressed mint should have data");
    
    // Deserialize and verify the compressed mint
    let compressed_mint: CompressedMint =
        BorshDeserialize::deserialize(&mut compressed_mint_account.data.unwrap().data.as_slice())
            .expect("Failed to deserialize compressed mint");
    
    assert_eq!(compressed_mint.decimals, decimals, "Decimals should match");
    assert_eq!(compressed_mint.mint_authority, Some(mint_authority.to_bytes().into()), "Mint authority should match");
    assert_eq!(compressed_mint.freeze_authority, Some(freeze_authority.to_bytes().into()), "Freeze authority should match");
    
    println!("✅ Compressed mint created and verified successfully");

    // Create test recipients
    let recipient_1 = Keypair::new();
    let recipient_2 = Keypair::new();

    let recipients = vec![
        Recipient {
            recipient: recipient_1.pubkey().into(),
            amount: 1000 * 10_u64.pow(decimals as u32), // 1000 tokens
        },
        Recipient {
            recipient: recipient_2.pubkey().into(),
            amount: 500 * 10_u64.pow(decimals as u32), // 500 tokens
        },
    ];

    println!("Recipients:");
    for (i, recipient) in recipients.iter().enumerate() {
        println!(
            "  {}: {} tokens to {}",
            i + 1,
            recipient.amount / 10_u64.pow(decimals as u32),
            Pubkey::from(recipient.recipient)
        );
    }

    // Mint tokens to the recipients
    mint_tokens(
        &mut rpc,
        &mint_authority_keypair,
        recipients.clone(),
        compressed_mint_address,
        None, // No lamports for this example
        &payer,
    )
    .await
    .unwrap();

    // Give the indexer a moment to process the minted tokens
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Get the SPL mint address (tokens are indexed under this, not the compressed mint address)
    let (spl_mint_address, _) = find_spl_mint_address(&mint_seed.pubkey());

    // Assert the tokens were minted to recipients
    for (i, recipient) in recipients.iter().enumerate() {
        let recipient_pubkey = Pubkey::from(recipient.recipient);
        
        // Get token balances for this recipient
        let token_balances = rpc
            .indexer()
            .unwrap()
            .get_compressed_token_balances_by_owner_v2(&recipient_pubkey, None, None)
            .await
            .expect(&format!("Failed to get token balances for recipient {}", i + 1));
        
        // Each recipient should have exactly one balance with the correct amount
        assert_eq!(token_balances.value.items.len(), 1, "Recipient {} should have exactly one token balance", i + 1);
        let balance = &token_balances.value.items[0];
        assert_eq!(balance.mint, spl_mint_address, "Recipient {} balance should be for our mint", i + 1);
        assert_eq!(balance.balance, recipient.amount, "Recipient {} should have the correct token balance", i + 1);
    }
}

pub async fn create_mint<R: Rpc + Indexer>(
    rpc: &mut R,
    mint_seed: &Keypair,
    decimals: u8,
    mint_authority: &Keypair,
    freeze_authority: Option<Pubkey>,
    metadata: Option<TokenMetadataInstructionData>,
    payer: &Keypair,
) -> Result<[u8; 32], RpcError> {
    // Get address tree and output queue from RPC
    let address_tree_pubkey = rpc.get_address_tree_v2().tree;

    let output_queue = rpc.get_random_state_tree_info()?.queue;

    // Derive compressed mint address using utility function
    let compressed_mint_address =
        derive_compressed_mint_address(&mint_seed.pubkey(), &address_tree_pubkey);

    // Find mint bump for the instruction
    let (_spl_mint, mint_bump) = find_spl_mint_address(&mint_seed.pubkey());

    // Get validity proof for address creation
    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![light_client::indexer::AddressWithTree {
                address: compressed_mint_address,
                tree: address_tree_pubkey,
            }],
            None,
        )
        .await?
        .value;

    // Create instruction data for the ctoken-minter program
    let inputs = CreateCompressedMintInstructionData {
        decimals,
        freeze_authority,
        proof: rpc_result.proof.0.unwrap(),
        mint_bump,
        address_merkle_tree_root_index: rpc_result.addresses[0].root_index,
        version: 0,
        metadata,
        compressed_mint_address,
    };
    // Create Anchor accounts struct
    let accounts = ctoken_minter::accounts::CreateCompressedMint {
        payer: payer.pubkey(),
        mint_authority: mint_authority.pubkey(),
    };

    // Create remaining accounts for the CPI
    let remaining_accounts = get_create_compressed_mint_instruction_account_metas(
        CreateCompressedMintMetaConfig::new_client(
            mint_seed.pubkey(),
            address_tree_pubkey,
            output_queue,
        ),
    );

    // Create the instruction
    let instruction_data = ctoken_minter::instruction::CreateCompressedMint { inputs };
    let ix = solana_sdk::instruction::Instruction {
        program_id: CTOKEN_MINTER_ID,
        accounts: [accounts.to_account_metas(None), remaining_accounts].concat(),
        data: instruction_data.data(),
    };

    // Determine signers (deduplicate if mint_signer and payer are the same)
    let mut signers = vec![payer, mint_authority];
    if mint_seed.pubkey() != payer.pubkey() {
        signers.push(mint_seed);
    }

    // Send the transaction
    rpc.create_and_send_transaction(&[ix], &payer.pubkey(), &signers)
        .await?;

    // Return the compressed mint address
    Ok(compressed_mint_address)
}

pub async fn mint_tokens<R: Rpc + Indexer>(
    rpc: &mut R,
    mint_authority: &Keypair,
    recipients: Vec<Recipient>,
    compressed_mint_address: [u8; 32],
    lamports: Option<u64>,
    payer: &Keypair,
) -> Result<Signature, RpcError> {
    let state_tree_info = rpc.get_random_state_tree_info()?;

    // Get the actual compressed mint account from the indexer
    let compressed_mint_account = rpc
        .indexer()
        .unwrap()
        .get_compressed_account(compressed_mint_address, None)
        .await
        .map_err(|e| {
            RpcError::CustomError(format!("Failed to get compressed mint account: {}", e))
        })?
        .value;

    // Deserialize the compressed mint
    let compressed_mint: CompressedMint =
        BorshDeserialize::deserialize(&mut compressed_mint_account.data.unwrap().data.as_slice())
            .map_err(|e| {
            RpcError::CustomError(format!("Failed to deserialize compressed mint: {}", e))
        })?;

    // Get validity proof for the compressed mint account
    let rpc_result = rpc
        .get_validity_proof(vec![compressed_mint_account.hash], vec![], None)
        .await
        .map_err(|e| RpcError::CustomError(format!("Failed to get validity proof: {}", e)))?
        .value;

    // Create compressed mint inputs using real data from the compressed mint account
    let compressed_mint_inputs = CompressedMintInputs {
        root_index: rpc_result.accounts[0]
            .root_index
            .root_index()
            .unwrap_or_default(),
        prove_by_index: rpc_result.accounts[0].root_index.root_index().is_none(),
        leaf_index: rpc_result.accounts[0].leaf_index as u32,
        address: compressed_mint_address,
        compressed_mint_input: compressed_mint,
    };

    // Create instruction data
    let inputs = MintCompressedTokensInstructionData {
        validity_proof: rpc_result.proof,
        compressed_mint_inputs,
        recipients,
        lamports,
    };

    // Create Anchor accounts struct
    let accounts = ctoken_minter::accounts::MintCompressedTokens {
        payer: payer.pubkey(),
        mint_authority: mint_authority.pubkey(),
    };

    // Use SDK function to generate account metas
    let meta_config = MintToCompressedMetaConfig::new_client(
        compressed_mint_account.tree_info.tree,  // state_merkle_tree
        state_tree_info.queue,                   // output_queue
        state_tree_info.tree,                    // state_tree_pubkey
        compressed_mint_account.tree_info.tree,  // compressed_mint_tree
        compressed_mint_account.tree_info.queue, // compressed_mint_queue
        lamports.is_some(),
    );

    let remaining_accounts = get_mint_to_compressed_instruction_account_metas(meta_config);

    // Create the instruction
    let instruction_data = ctoken_minter::instruction::MintCompressedTokens { inputs };
    let ix = solana_sdk::instruction::Instruction {
        program_id: CTOKEN_MINTER_ID,
        accounts: [accounts.to_account_metas(None), remaining_accounts].concat(),
        data: instruction_data.data(),
    };

    // Determine signers
    let mut signers = vec![payer];
    if mint_authority.pubkey() != payer.pubkey() {
        signers.push(mint_authority);
    }

    // Send the transaction
    rpc.create_and_send_transaction(&[ix], &payer.pubkey(), &signers)
        .await
}
