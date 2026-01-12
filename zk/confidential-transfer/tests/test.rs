use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::convert_proof;
use light_client::indexer::CompressedAccount;
use light_hasher::{
    hash_to_field_size::hashv_to_bn254_field_size_be_const_array, Hasher, Poseidon,
};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{PackedAccounts, SystemAccountMetaConfig},
};
use num_bigint::BigUint;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
};
use std::collections::HashMap;

use confidential_transfer::{BALANCE, NULLIFIER};

// Balance commitment: Poseidon(amount, blinding)
#[derive(Debug, Clone)]
struct BalanceSecret {
    pub amount: u64,
    pub blinding: [u8; 32],
    pub commitment: [u8; 32],
}

impl BalanceSecret {
    pub fn new(amount: u64) -> Self {
        // Generate random blinding factor
        let mut blinding = [0u8; 32];
        let random_bytes: Vec<u8> = (0..31).map(|_| rand::random::<u8>()).collect();
        blinding[1..32].copy_from_slice(&random_bytes);

        // commitment = Poseidon(amount, blinding)
        let mut amount_bytes = [0u8; 32];
        amount_bytes[24..].copy_from_slice(&amount.to_be_bytes());

        let commitment = Poseidon::hashv(&[&amount_bytes, &blinding]).unwrap();

        Self {
            amount,
            blinding,
            commitment,
        }
    }

    pub fn compute_nullifier(&self) -> [u8; 32] {
        // nullifier = Poseidon(commitment, blinding)
        Poseidon::hashv(&[&self.commitment, &self.blinding]).unwrap()
    }
}

#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(privatetransfer);

/// Test that the ZK proof generation and verification works correctly
#[tokio::test]
async fn test_private_transfer_circuit() {
    // Initialize test environment
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("confidential_transfer", confidential_transfer::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();
    let address_tree_info = rpc.get_address_tree_v2();

    // Create a balance commitment (simulating deposit)
    let initial_balance = BalanceSecret::new(1_000_000);

    // Derive commitment address
    let (commitment_address, _) = derive_address(
        &[BALANCE, initial_balance.commitment.as_slice()],
        &address_tree_info.tree,
        &confidential_transfer::ID,
    );

    // Create balance commitment directly (simulating deposit without token transfer)
    create_balance_commitment(
        &mut rpc,
        &payer,
        &commitment_address,
        address_tree_info.clone(),
        initial_balance.commitment,
    )
    .await
    .unwrap();

    // Verify commitment account was created
    let program_accounts = rpc
        .get_compressed_accounts_by_owner(&confidential_transfer::ID, None, None)
        .await
        .unwrap();
    assert_eq!(
        program_accounts.value.items.len(),
        1,
        "Balance account should be created"
    );

    // Get the created balance account by address (more reliable for merkle proofs)
    let balance_account = rpc
        .get_compressed_account(commitment_address, None)
        .await
        .unwrap()
        .value
        .expect("Balance account not found by address");

    // Prepare transfer
    let transfer_amount = 300_000u64;
    let receiver_balance = BalanceSecret::new(transfer_amount);

    // Get merkle proof for the balance account
    let proofs = rpc
        .get_multiple_compressed_account_proofs(vec![balance_account.hash], None)
        .await
        .unwrap()
        .value
        .items;

    assert!(
        !proofs.is_empty(),
        "Should have merkle proof for balance account"
    );
    let merkle_proof = &proofs[0];
    let leaf_index = merkle_proof.leaf_index as u32;
    let merkle_proof_hashes = &merkle_proof.proof;
    let merkle_root = merkle_proof.root;
    let root_index = (merkle_proof.root_seq % 2400) as u16;
    let state_tree = merkle_proof.merkle_tree;

    let nullifier = initial_balance.compute_nullifier();

    // Generate ZK proof for transfer
    let transfer_proof = generate_transfer_proof(
        &balance_account,
        &state_tree,
        leaf_index,
        merkle_proof_hashes,
        &merkle_root,
        &initial_balance,
        transfer_amount,
        &receiver_balance,
        &nullifier,
    );

    // Derive addresses for new accounts
    let (nullifier_address, _) = derive_address(
        &[NULLIFIER, nullifier.as_slice()],
        &address_tree_info.tree,
        &confidential_transfer::ID,
    );

    let (receiver_address, _) = derive_address(
        &[BALANCE, receiver_balance.commitment.as_slice()],
        &address_tree_info.tree,
        &confidential_transfer::ID,
    );

    // Execute transfer
    private_transfer(
        &mut rpc,
        &payer,
        &nullifier_address,
        &receiver_address,
        address_tree_info.clone(),
        &state_tree,
        root_index,
        transfer_proof,
        nullifier,
        receiver_balance.commitment,
    )
    .await
    .unwrap();

    // Verify 3 accounts exist: original commitment + nullifier + receiver commitment
    let final_accounts = rpc
        .get_compressed_accounts_by_owner(&confidential_transfer::ID, None, None)
        .await
        .unwrap();
    assert_eq!(final_accounts.value.items.len(), 3);
}

/// Create a balance commitment account directly (simulates deposit without token transfer)
async fn create_balance_commitment<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    commitment: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(confidential_transfer::ID);
    remaining_accounts.add_system_accounts(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: *address,
                tree: address_tree_info.tree,
            }],
            None,
        )
        .await?
        .value;

    let packed_address_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees;

    // Use V1 state tree for immediate merkle tree insertion (V2 uses output queues)
    let output_state_tree_index = rpc
        .get_random_state_tree_info_v1()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    // Use placeholder mint hash (all zeros) for testing
    let mint_hashed = hashv_to_bn254_field_size_be_const_array::<2>(&[&[0u8; 32]]).unwrap();

    let instruction_data = confidential_transfer::instruction::CreateBalanceCommitment {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        commitment,
        mint_hashed,
    };

    let accounts = confidential_transfer::accounts::CreateBalanceCommitmentAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: confidential_transfer::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn private_transfer<R>(
    rpc: &mut R,
    payer: &Keypair,
    nullifier_address: &[u8; 32],
    receiver_address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    input_merkle_tree: &Pubkey,
    input_root_index: u16,
    transfer_proof: light_compressed_account::instruction_data::compressed_proof::CompressedProof,
    nullifier: [u8; 32],
    receiver_commitment: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(confidential_transfer::ID);
    remaining_accounts.add_system_accounts(config)?;

    // Get validity proof for both new addresses (nullifier + receiver)
    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![
                AddressWithTree {
                    address: *nullifier_address,
                    tree: address_tree_info.tree,
                },
                AddressWithTree {
                    address: *receiver_address,
                    tree: address_tree_info.tree,
                },
            ],
            None,
        )
        .await?
        .value;

    let packed_address_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees;

    let output_state_tree_index = rpc
        .get_random_state_tree_info_v1()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let mint_hashed = hashv_to_bn254_field_size_be_const_array::<2>(&[&[0u8; 32]]).unwrap();

    let instruction_data = confidential_transfer::instruction::Transfer {
        proof: rpc_result.proof,
        nullifier_address_tree_info: packed_address_tree_accounts[0],
        receiver_address_tree_info: packed_address_tree_accounts[1],
        output_state_tree_index,
        input_root_index,
        transfer_proof,
        nullifier,
        receiver_commitment,
        mint_hashed,
    };

    let accounts = confidential_transfer::accounts::TransferPrivate {
        signer: payer.pubkey(),
        input_merkle_tree: *input_merkle_tree,
    };

    let instruction = Instruction {
        program_id: confidential_transfer::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

fn generate_transfer_proof(
    balance_account: &CompressedAccount,
    merkle_tree_pubkey: &Pubkey,
    leaf_index: u32,
    merkle_proof_hashes: &[[u8; 32]],
    merkle_root: &[u8; 32],
    sender_balance: &BalanceSecret,
    transfer_amount: u64,
    receiver_balance: &BalanceSecret,
    nullifier: &[u8; 32],
) -> light_compressed_account::instruction_data::compressed_proof::CompressedProof {
    // Create new_sender_balance for the proof (circuit still constrains it, but not public)
    let new_sender_balance = BalanceSecret::new(sender_balance.amount - transfer_amount);
    let zkey_path = "./build/private_transfer_final.zkey".to_string();
    let mut proof_inputs = HashMap::new();

    let discriminator = if let Some(ref data) = balance_account.data {
        data.discriminator
    } else {
        [0u8; 8]
    };

    let owner_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[confidential_transfer::ID.as_ref()])
            .unwrap();
    let merkle_tree_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[merkle_tree_pubkey.as_ref()]).unwrap();
    let mint_hashed = hashv_to_bn254_field_size_be_const_array::<2>(&[&[0u8; 32]]).unwrap();

    // Compute data_hash: Poseidon(mint_hashed, commitment)
    // This matches LightHasher derive without #[hash] attributes
    let computed_data_hash = Poseidon::hashv(&[&mint_hashed, &sender_balance.commitment]).unwrap();

    // Verify data_hash matches what SDK stored
    let stored_data_hash = balance_account.data.as_ref().unwrap().data_hash;
    assert_eq!(computed_data_hash, stored_data_hash, "Data hash mismatch");

    // Verify account hash matches what circuit will compute
    // SDK format: 32-byte array with leaf_index in LE at [28..32]
    let mut leaf_index_bytes = [0u8; 32];
    leaf_index_bytes[28..32].copy_from_slice(&(balance_account.leaf_index as u32).to_le_bytes());

    // SDK format: 32-byte array with discriminator at [24..32] and prefix 2 at [23]
    let mut discriminator_bytes = [0u8; 32];
    discriminator_bytes[24..32].copy_from_slice(&discriminator);
    discriminator_bytes[23] = 2;

    let computed_leaf_hash = Poseidon::hashv(&[
        owner_hashed.as_slice(),
        leaf_index_bytes.as_slice(),
        merkle_tree_hashed.as_slice(),
        balance_account.address.as_ref().unwrap().as_ref(),
        discriminator_bytes.as_slice(),
        computed_data_hash.as_slice(),
    ])
    .unwrap();

    assert_eq!(
        computed_leaf_hash, balance_account.hash,
        "Leaf hash mismatch"
    );

    // Public inputs (must match circuit order)
    proof_inputs.insert(
        "owner_hashed".to_string(),
        vec![BigUint::from_bytes_be(&owner_hashed).to_string()],
    );
    proof_inputs.insert(
        "merkle_tree_hashed".to_string(),
        vec![BigUint::from_bytes_be(&merkle_tree_hashed).to_string()],
    );
    proof_inputs.insert(
        "discriminator".to_string(),
        vec![BigUint::from_bytes_be(&discriminator).to_string()],
    );
    proof_inputs.insert(
        "mint_hashed".to_string(),
        vec![BigUint::from_bytes_be(&mint_hashed).to_string()],
    );
    proof_inputs.insert(
        "expectedRoot".to_string(),
        vec![BigUint::from_bytes_be(merkle_root).to_string()],
    );
    proof_inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(nullifier).to_string()],
    );
    proof_inputs.insert(
        "receiver_commitment".to_string(),
        vec![BigUint::from_bytes_be(&receiver_balance.commitment).to_string()],
    );

    // Private inputs (including new_sender_commitment which is constrained but not public)
    proof_inputs.insert(
        "new_sender_commitment".to_string(),
        vec![BigUint::from_bytes_be(&new_sender_balance.commitment).to_string()],
    );

    let mut sender_amount_bytes = [0u8; 32];
    sender_amount_bytes[24..].copy_from_slice(&sender_balance.amount.to_be_bytes());
    proof_inputs.insert(
        "sender_amount".to_string(),
        vec![BigUint::from_bytes_be(&sender_amount_bytes).to_string()],
    );
    proof_inputs.insert(
        "sender_blinding".to_string(),
        vec![BigUint::from_bytes_be(&sender_balance.blinding).to_string()],
    );

    let mut transfer_amount_bytes = [0u8; 32];
    transfer_amount_bytes[24..].copy_from_slice(&transfer_amount.to_be_bytes());
    proof_inputs.insert(
        "transfer_amount".to_string(),
        vec![BigUint::from_bytes_be(&transfer_amount_bytes).to_string()],
    );

    proof_inputs.insert(
        "new_sender_blinding".to_string(),
        vec![BigUint::from_bytes_be(&new_sender_balance.blinding).to_string()],
    );
    proof_inputs.insert(
        "receiver_blinding".to_string(),
        vec![BigUint::from_bytes_be(&receiver_balance.blinding).to_string()],
    );

    // Account position
    proof_inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32]
        .copy_from_slice(&(balance_account.leaf_index as u32).to_le_bytes());
    proof_inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    let address = balance_account.address.unwrap_or([0u8; 32]);
    proof_inputs.insert(
        "address".to_string(),
        vec![BigUint::from_bytes_be(&address).to_string()],
    );

    // Merkle proof
    let path_elements: Vec<String> = merkle_proof_hashes
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    proof_inputs.insert("pathElements".to_string(), path_elements);

    // Generate proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();

    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(privatetransfer_witness),
        circuit_inputs,
        zkey_path.clone(),
    );

    let proof = match proof {
        Ok(p) => p,
        Err(e) => panic!("Proof generation failed: {:?}", e),
    };

    // Verify proof locally
    let verification_result =
        CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path.clone());

    let is_valid = verification_result.expect("Proof verification failed");
    assert!(is_valid, "Local proof verification should pass");

    // Convert to groth16-solana format and compress
    let (proof_a_uncompressed, proof_b_uncompressed, proof_c_uncompressed) =
        convert_proof(&proof.proof).expect("Failed to convert proof");

    use groth16_solana::proof_parser::circom_prover::convert_proof_to_compressed;
    let (proof_a, proof_b, proof_c) = convert_proof_to_compressed(
        &proof_a_uncompressed,
        &proof_b_uncompressed,
        &proof_c_uncompressed,
    )
    .expect("Failed to compress proof");

    light_compressed_account::instruction_data::compressed_proof::CompressedProof {
        a: proof_a,
        b: proof_b,
        c: proof_c,
    }
}
