use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::groth16::Groth16Verifier;
use groth16_solana::proof_parser::circom_prover::{convert_proof, convert_public_inputs};
use light_compressed_account::compressed_account::{CompressedAccount, CompressedAccountData};
use light_compressed_account::Pubkey;
use light_hasher::{
    hash_to_field_size::{hash_to_bn254_field_size_be, hashv_to_bn254_field_size_be_const_array},
    Hasher, Poseidon, Sha256,
};
use light_merkle_tree_reference::MerkleTree;
use num_bigint::BigUint;
use std::collections::HashMap;

// Link the generated witness library
#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(compressedaccountmerkleproof);

// Use the verifying key from the library
use zk_id::verifying_key::VERIFYINGKEY;

/// Helper function to add compressed account inputs to the circuit inputs HashMap
///
/// # Arguments
/// * `inputs` - Mutable reference to the HashMap that will be populated with circuit inputs
/// * `compressed_account` - The compressed account to convert to circuit inputs
/// * `merkle_tree_pubkey` - The public key of the Merkle tree
/// * `leaf_index` - The index of the leaf in the Merkle tree
/// * `issuer_pubkey` - The issuer's public key
/// * `credential_pubkey` - The credential public key (private input)
/// * `encrypted_data` - The encrypted data
fn add_compressed_account_to_circuit_inputs(
    inputs: &mut HashMap<String, Vec<String>>,
    compressed_account: &CompressedAccount,
    merkle_tree_pubkey: &Pubkey,
    leaf_index: u32,
    issuer_pubkey: &Pubkey,
    credential_pubkey: &Pubkey,
    encrypted_data: &[u8],
) {
    // Extract data from compressed account
    let owner = compressed_account.owner;
    let discriminator = if let Some(ref data) = compressed_account.data {
        data.discriminator
    } else {
        [0u8; 8]
    };

    // Hash values for circuit - use 2-round hash like on-chain
    let owner_hashed = hash_to_bn254_field_size_be(owner.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());
    let issuer_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[issuer_pubkey.as_ref()]).unwrap();
    let credential_pubkey_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[credential_pubkey.as_ref()]).unwrap();

    // Hash encrypted_data with SHA256 and truncate (set first byte to 0)
    // Include length prefix like in the main test
    let mut hash_input = Vec::new();
    hash_input.extend_from_slice((encrypted_data.len() as u32).to_le_bytes().as_ref());
    hash_input.extend_from_slice(encrypted_data);
    let mut encrypted_data_hash = Sha256::hash(&hash_input).unwrap();
    encrypted_data_hash[0] = 0;

    // Compute public_data_hash (hash of issuer and credential pubkey)
    let public_data_hash = Poseidon::hashv(&[
        issuer_hashed.as_slice(),
        credential_pubkey_hashed.as_slice(),
    ])
    .unwrap();

    // Add all inputs to the HashMap
    inputs.insert(
        "owner_hashed".to_string(),
        vec![BigUint::from_bytes_be(&owner_hashed).to_string()],
    );
    inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    // Add account_leaf_index (same format as SDK: 32-byte array with value at [28..32] in LE)
    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32].copy_from_slice(&(leaf_index as u32).to_le_bytes());
    inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    // Add address field - use the address from the compressed account
    let address = compressed_account.address.unwrap_or([0u8; 32]);
    inputs.insert(
        "address".to_string(),
        vec![BigUint::from_bytes_be(&address).to_string()],
    );

    inputs.insert(
        "merkle_tree_hashed".to_string(),
        vec![BigUint::from_bytes_be(&merkle_tree_hashed).to_string()],
    );
    inputs.insert(
        "discriminator".to_string(),
        vec![BigUint::from_bytes_be(&discriminator).to_string()],
    );
    inputs.insert(
        "issuer_hashed".to_string(),
        vec![BigUint::from_bytes_be(&issuer_hashed).to_string()],
    );
    inputs.insert(
        "credential_pubkey_hashed".to_string(),
        vec![BigUint::from_bytes_be(&credential_pubkey_hashed).to_string()],
    );
    inputs.insert(
        "encrypted_data_hash".to_string(),
        vec![BigUint::from_bytes_be(&encrypted_data_hash).to_string()],
    );
    inputs.insert(
        "public_encrypted_data_hash".to_string(),
        vec![BigUint::from_bytes_be(&encrypted_data_hash).to_string()],
    );
    inputs.insert(
        "public_data_hash".to_string(),
        vec![BigUint::from_bytes_be(&public_data_hash).to_string()],
    );
}

/// Helper function to add Merkle proof inputs to the circuit inputs HashMap
///
/// # Arguments
/// * `inputs` - Mutable reference to the HashMap that will be populated with circuit inputs
/// * `merkle_proof_hashes` - Vector of Merkle proof path elements (32-byte hashes)
/// * `merkle_root` - The expected Merkle root (32-byte hash)
fn add_merkle_proof_to_circuit_inputs(
    inputs: &mut HashMap<String, Vec<String>>,
    merkle_proof_hashes: &[[u8; 32]],
    merkle_root: &[u8; 32],
) {
    // Convert Merkle proof path elements to BigUint strings
    let path_elements: Vec<String> = merkle_proof_hashes
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    inputs.insert("pathElements".to_string(), path_elements);

    // Convert expected root to BigUint string
    let expected_root_bigint = BigUint::from_bytes_be(merkle_root);
    inputs.insert(
        "expectedRoot".to_string(),
        vec![expected_root_bigint.to_string()],
    );
}

#[test]
fn test_compressed_account_merkle_proof_circuit() {
    let zkey_path = "./build/compressed_account_merkle_proof_final.zkey".to_string();

    // Create test data
    let owner = Pubkey::new_from_array([1u8; 32]);
    let merkle_tree_pubkey = Pubkey::new_from_array([2u8; 32]);
    let leaf_index: u32 = 0;
    let issuer_pubkey = Pubkey::new_from_array([4u8; 32]);
    let credential_pubkey = Pubkey::new_from_array([5u8; 32]);
    let encrypted_data = vec![6u8; 64];
    let mut address = [3u8; 32];
    address[0] = 0; // Ensure first byte is 0

    // Compute data_hash as hash of issuer and credential - use 2-round hash
    let issuer_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[issuer_pubkey.as_ref()]).unwrap();
    let credential_pubkey_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[credential_pubkey.as_ref()]).unwrap();
    let data_hash = Poseidon::hashv(&[
        issuer_hashed.as_slice(),
        credential_pubkey_hashed.as_slice(),
    ])
    .unwrap();

    let compressed_account = CompressedAccount {
        owner,
        lamports: 0,
        address: Some(address),
        data: Some(CompressedAccountData {
            discriminator: [1u8; 8],
            data: vec![],
            data_hash,
        }),
    };

    // Create Merkle tree and get proof
    let compressed_account_hash = compressed_account
        .hash(&merkle_tree_pubkey, &leaf_index, false)
        .unwrap();

    let mut merkle_tree = MerkleTree::<Poseidon>::new(26, 0);
    merkle_tree.append(&compressed_account_hash).unwrap();

    let merkle_proof_hashes = merkle_tree
        .get_proof_of_leaf(leaf_index as usize, false)
        .unwrap();
    let merkle_root = merkle_tree.root();

    // Build circuit inputs
    let mut proof_inputs = HashMap::new();
    add_compressed_account_to_circuit_inputs(
        &mut proof_inputs,
        &compressed_account,
        &merkle_tree_pubkey,
        leaf_index,
        &issuer_pubkey,
        &credential_pubkey,
        &encrypted_data,
    );
    add_merkle_proof_to_circuit_inputs(&mut proof_inputs, &merkle_proof_hashes, &merkle_root);

    // Generate and verify proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(compressedaccountmerkleproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof, zkey_path)
        .expect("Proof verification failed");

    assert!(is_valid, "Proof should be valid");
}

#[test]
fn test_invalid_proof_rejected() {
    let zkey_path = "./build/compressed_account_merkle_proof_final.zkey".to_string();

    // Create test data
    let owner = Pubkey::new_from_array([1u8; 32]);
    let merkle_tree_pubkey = Pubkey::new_from_array([2u8; 32]);
    let leaf_index: u32 = 0;
    let issuer_pubkey = Pubkey::new_from_array([4u8; 32]);
    let credential_pubkey = Pubkey::new_from_array([5u8; 32]);
    let encrypted_data = vec![6u8; 64];

    // Compute data_hash as hash of issuer and credential - use 2-round hash
    let issuer_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[issuer_pubkey.as_ref()]).unwrap();
    let credential_pubkey_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[credential_pubkey.as_ref()]).unwrap();
    let data_hash = Poseidon::hashv(&[
        issuer_hashed.as_slice(),
        credential_pubkey_hashed.as_slice(),
    ])
    .unwrap();

    let compressed_account = CompressedAccount {
        owner,
        lamports: 0,
        address: None,
        data: Some(CompressedAccountData {
            discriminator: [1u8; 8],
            data: vec![],
            data_hash,
        }),
    };

    // Create Merkle tree and get proof
    let compressed_account_hash = compressed_account
        .hash(&merkle_tree_pubkey, &leaf_index, false)
        .unwrap();

    let mut merkle_tree = MerkleTree::<Poseidon>::new(26, 0);
    merkle_tree.append(&compressed_account_hash).unwrap();
    let merkle_proof_hashes = merkle_tree
        .get_proof_of_leaf(leaf_index as usize, false)
        .unwrap();

    // Build circuit inputs with INVALID root
    let mut proof_inputs = HashMap::new();
    add_compressed_account_to_circuit_inputs(
        &mut proof_inputs,
        &compressed_account,
        &merkle_tree_pubkey,
        leaf_index,
        &issuer_pubkey,
        &credential_pubkey,
        &encrypted_data,
    );

    let invalid_root = [0u8; 32];
    add_merkle_proof_to_circuit_inputs(&mut proof_inputs, &merkle_proof_hashes, &invalid_root);

    // Generate proof (succeeds even with wrong root)
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(compressedaccountmerkleproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation should succeed");

    // Verify proof (should fail due to constraint violation)
    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof, zkey_path)
        .expect("Verification should return a result");

    assert!(!is_valid, "Proof should be invalid with wrong root");
}

#[test]
fn test_groth16_solana_verification() {
    let zkey_path = "./build/compressed_account_merkle_proof_final.zkey".to_string();

    // Create test data
    let owner = Pubkey::new_from_array([1u8; 32]);
    let merkle_tree_pubkey = Pubkey::new_from_array([2u8; 32]);
    let leaf_index: u32 = 0;
    let issuer_pubkey = Pubkey::new_from_array([4u8; 32]);
    let credential_pubkey = Pubkey::new_from_array([5u8; 32]);
    let encrypted_data = vec![6u8; 64];
    let mut address = [3u8; 32];
    address[0] = 0; // Ensure first byte is 0

    // Compute data_hash as hash of issuer and credential - use 2-round hash
    let issuer_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[issuer_pubkey.as_ref()]).unwrap();
    let credential_pubkey_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[credential_pubkey.as_ref()]).unwrap();
    let data_hash = Poseidon::hashv(&[
        issuer_hashed.as_slice(),
        credential_pubkey_hashed.as_slice(),
    ])
    .unwrap();

    let compressed_account = CompressedAccount {
        owner,
        lamports: 0,
        address: Some(address),
        data: Some(CompressedAccountData {
            discriminator: [1u8; 8],
            data: vec![],
            data_hash,
        }),
    };

    // Create Merkle tree and get proof
    let compressed_account_hash = compressed_account
        .hash(&merkle_tree_pubkey, &leaf_index, false)
        .unwrap();

    let mut merkle_tree = MerkleTree::<Poseidon>::new(26, 0);
    merkle_tree.append(&compressed_account_hash).unwrap();

    let merkle_proof_hashes = merkle_tree
        .get_proof_of_leaf(leaf_index as usize, false)
        .unwrap();
    let merkle_root = merkle_tree.root();

    // Build circuit inputs
    let mut proof_inputs = HashMap::new();
    add_compressed_account_to_circuit_inputs(
        &mut proof_inputs,
        &compressed_account,
        &merkle_tree_pubkey,
        leaf_index,
        &issuer_pubkey,
        &credential_pubkey,
        &encrypted_data,
    );
    add_merkle_proof_to_circuit_inputs(&mut proof_inputs, &merkle_proof_hashes, &merkle_root);

    // Generate proof with circom-prover
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(compressedaccountmerkleproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    // First verify with circom-prover
    let is_valid_circom = CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path)
        .expect("Circom verification failed");
    assert!(is_valid_circom, "Proof should be valid with circom-prover");

    // Convert proof and public inputs to groth16-solana format
    let (proof_a, proof_b, proof_c) = convert_proof(&proof.proof).expect("Failed to convert proof");
    let public_inputs: [[u8; 32]; 7] = convert_public_inputs(&proof.pub_inputs);

    // Verify with groth16-solana
    let mut verifier =
        Groth16Verifier::new(&proof_a, &proof_b, &proof_c, &public_inputs, &VERIFYINGKEY)
            .expect("Failed to create verifier");

    verifier.verify().expect("Groth16 verification failed");
}
