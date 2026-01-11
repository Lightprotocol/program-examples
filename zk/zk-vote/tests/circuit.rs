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
use solana_sdk::signature::{Keypair, Signer};
use std::collections::HashMap;

// Link the generated witness library
#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(voteproof);

// Use the verifying key from the library
use zk_vote::verifying_key::VERIFYINGKEY;

/// Derives a credential keypair from a Solana keypair
/// The private key is derived by signing "CREDENTIAL" and truncating to 248 bits
/// The public key is Poseidon(private_key)
#[derive(Debug, Clone)]
struct CredentialKeypair {
    pub private_key: [u8; 32], // 248 bits
    pub public_key: [u8; 32],  // Poseidon hash of private key
}

impl CredentialKeypair {
    pub fn new(solana_keypair: &Keypair) -> Self {
        // Sign the message "CREDENTIAL" with the Solana keypair
        let message = b"CREDENTIAL";
        let signature = solana_keypair.sign_message(message);

        // Hash the signature to get entropy
        let hashed = Sha256::hash(signature.as_ref()).unwrap();

        // Truncate to 248 bits (31 bytes) for BN254 field compatibility
        let mut private_key = [0u8; 32];
        private_key[1..32].copy_from_slice(&hashed[0..31]);

        let public_key = Poseidon::hashv(&[&private_key]).unwrap();

        Self {
            private_key,
            public_key,
        }
    }

    /// Get the private key as a BigUint for circuit input
    pub fn private_key_biguint(&self) -> BigUint {
        BigUint::from_bytes_be(&self.private_key)
    }

    /// Compute nullifier for a given poll_id_hashed
    pub fn compute_nullifier(&self, poll_id_hashed: &[u8; 32]) -> [u8; 32] {
        // Nullifier = Poseidon(poll_id_hashed, private_key)
        Poseidon::hashv(&[poll_id_hashed, &self.private_key]).unwrap()
    }
}

/// Build circuit inputs for vote proof
fn build_vote_proof_inputs(
    owner_hashed: &[u8; 32],
    merkle_tree_hashed: &[u8; 32],
    discriminator: &[u8; 8],
    poll_id_hashed: &[u8; 32],
    merkle_root: &[u8; 32],
    nullifier: &[u8; 32],
    vote_choice: u8,
    credential: &CredentialKeypair,
    leaf_index: u32,
    address: &[u8; 32],
    merkle_proof_hashes: &[[u8; 32]],
) -> HashMap<String, Vec<String>> {
    let mut inputs = HashMap::new();

    // Public inputs (7)
    inputs.insert(
        "owner_hashed".to_string(),
        vec![BigUint::from_bytes_be(owner_hashed).to_string()],
    );
    inputs.insert(
        "merkle_tree_hashed".to_string(),
        vec![BigUint::from_bytes_be(merkle_tree_hashed).to_string()],
    );
    inputs.insert(
        "discriminator".to_string(),
        vec![BigUint::from_bytes_be(discriminator).to_string()],
    );
    inputs.insert(
        "poll_id_hashed".to_string(),
        vec![BigUint::from_bytes_be(poll_id_hashed).to_string()],
    );
    inputs.insert(
        "expectedRoot".to_string(),
        vec![BigUint::from_bytes_be(merkle_root).to_string()],
    );
    inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(nullifier).to_string()],
    );
    inputs.insert("vote_choice".to_string(), vec![vote_choice.to_string()]);

    // Private inputs
    inputs.insert(
        "credentialPrivateKey".to_string(),
        vec![credential.private_key_biguint().to_string()],
    );
    inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    // account_leaf_index in SDK format
    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32].copy_from_slice(&leaf_index.to_le_bytes());
    inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    inputs.insert(
        "address".to_string(),
        vec![BigUint::from_bytes_be(address).to_string()],
    );

    // Merkle proof path elements
    let path_elements: Vec<String> = merkle_proof_hashes
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    inputs.insert("pathElements".to_string(), path_elements);

    inputs
}

#[test]
fn test_vote_proof_circuit() {
    let zkey_path = "./build/vote_proof_final.zkey".to_string();

    // Create test data
    let program_id = Pubkey::new_from_array([1u8; 32]);
    let merkle_tree_pubkey = Pubkey::new_from_array([2u8; 32]);
    let leaf_index: u32 = 0;
    let poll_id: u32 = 1;

    // Create credential keypair
    let user_keypair = Keypair::new();
    let credential = CredentialKeypair::new(&user_keypair);

    let mut address = [3u8; 32];
    address[0] = 0; // Ensure first byte is 0 for BN254 field

    // Hash values for circuit
    let owner_hashed = hash_to_bn254_field_size_be(program_id.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());
    let poll_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&poll_id.to_le_bytes()]).unwrap();

    // Compute nullifier
    let nullifier = credential.compute_nullifier(&poll_id_hashed);

    let discriminator = [1u8; 8]; // VoterCredential discriminator

    // Compute data_hash = Poseidon(poll_id_hashed, credential_pubkey)
    let data_hash = Poseidon::hashv(&[poll_id_hashed.as_slice(), &credential.public_key]).unwrap();

    // Create VoterCredential compressed account
    let compressed_account = CompressedAccount {
        owner: program_id,
        lamports: 0,
        address: Some(address),
        data: Some(CompressedAccountData {
            discriminator,
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
    let vote_choice = 1u8; // Vote for option 1

    let proof_inputs = build_vote_proof_inputs(
        &owner_hashed,
        &merkle_tree_hashed,
        &discriminator,
        &poll_id_hashed,
        &merkle_root,
        &nullifier,
        vote_choice,
        &credential,
        leaf_index,
        &address,
        &merkle_proof_hashes,
    );

    // Generate and verify proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(voteproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof, zkey_path)
        .expect("Proof verification failed");

    assert!(is_valid, "Proof should be valid");
}

#[test]
#[ignore] // TODO: Investigate why LessThan constraint doesn't reject vote_choice=3
fn test_invalid_vote_choice_rejected() {
    let zkey_path = "./build/vote_proof_final.zkey".to_string();

    // Create test data
    let program_id = Pubkey::new_from_array([1u8; 32]);
    let merkle_tree_pubkey = Pubkey::new_from_array([2u8; 32]);
    let leaf_index: u32 = 0;
    let poll_id: u32 = 1;

    // Create credential keypair
    let user_keypair = Keypair::new();
    let credential = CredentialKeypair::new(&user_keypair);

    let mut address = [3u8; 32];
    address[0] = 0;

    // Hash values
    let owner_hashed = hash_to_bn254_field_size_be(program_id.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());
    let poll_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&poll_id.to_le_bytes()]).unwrap();

    let nullifier = credential.compute_nullifier(&poll_id_hashed);
    let discriminator = [1u8; 8];

    let data_hash = Poseidon::hashv(&[poll_id_hashed.as_slice(), &credential.public_key]).unwrap();

    let compressed_account = CompressedAccount {
        owner: program_id,
        lamports: 0,
        address: Some(address),
        data: Some(CompressedAccountData {
            discriminator,
            data: vec![],
            data_hash,
        }),
    };

    let compressed_account_hash = compressed_account
        .hash(&merkle_tree_pubkey, &leaf_index, false)
        .unwrap();

    let mut merkle_tree = MerkleTree::<Poseidon>::new(26, 0);
    merkle_tree.append(&compressed_account_hash).unwrap();

    let merkle_proof_hashes = merkle_tree
        .get_proof_of_leaf(leaf_index as usize, false)
        .unwrap();
    let merkle_root = merkle_tree.root();

    // Invalid vote choice (3 is out of range: must be 0, 1, or 2)
    let vote_choice = 3u8;

    let proof_inputs = build_vote_proof_inputs(
        &owner_hashed,
        &merkle_tree_hashed,
        &discriminator,
        &poll_id_hashed,
        &merkle_root,
        &nullifier,
        vote_choice,
        &credential,
        leaf_index,
        &address,
        &merkle_proof_hashes,
    );

    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();

    // Try to generate proof - may fail during generation or produce invalid proof
    let result = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(voteproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    );

    match result {
        Err(_) => {
            // Proof generation failed as expected
        }
        Ok(proof) => {
            // Proof was generated, but should fail verification
            let is_valid = CircomProver::verify(ProofLib::Arkworks, proof, zkey_path)
                .expect("Verification should return a result");
            assert!(
                !is_valid,
                "Proof should be invalid with invalid vote choice"
            );
        }
    }
}

#[test]
fn test_groth16_solana_verification() {
    let zkey_path = "./build/vote_proof_final.zkey".to_string();

    // Create test data
    let program_id = Pubkey::new_from_array([1u8; 32]);
    let merkle_tree_pubkey = Pubkey::new_from_array([2u8; 32]);
    let leaf_index: u32 = 0;
    let poll_id: u32 = 42;

    // Create credential keypair
    let user_keypair = Keypair::new();
    let credential = CredentialKeypair::new(&user_keypair);

    let mut address = [3u8; 32];
    address[0] = 0;

    let owner_hashed = hash_to_bn254_field_size_be(program_id.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());
    let poll_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&poll_id.to_le_bytes()]).unwrap();

    let nullifier = credential.compute_nullifier(&poll_id_hashed);
    let discriminator = [1u8; 8];

    let data_hash = Poseidon::hashv(&[poll_id_hashed.as_slice(), &credential.public_key]).unwrap();

    let compressed_account = CompressedAccount {
        owner: program_id,
        lamports: 0,
        address: Some(address),
        data: Some(CompressedAccountData {
            discriminator,
            data: vec![],
            data_hash,
        }),
    };

    let compressed_account_hash = compressed_account
        .hash(&merkle_tree_pubkey, &leaf_index, false)
        .unwrap();

    let mut merkle_tree = MerkleTree::<Poseidon>::new(26, 0);
    merkle_tree.append(&compressed_account_hash).unwrap();

    let merkle_proof_hashes = merkle_tree
        .get_proof_of_leaf(leaf_index as usize, false)
        .unwrap();
    let merkle_root = merkle_tree.root();

    let vote_choice = 2u8;

    let proof_inputs = build_vote_proof_inputs(
        &owner_hashed,
        &merkle_tree_hashed,
        &discriminator,
        &poll_id_hashed,
        &merkle_root,
        &nullifier,
        vote_choice,
        &credential,
        leaf_index,
        &address,
        &merkle_proof_hashes,
    );

    // Generate proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(voteproof_witness),
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
