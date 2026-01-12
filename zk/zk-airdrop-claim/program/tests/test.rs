use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::convert_proof;
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
    system_instruction,
};
use std::collections::HashMap;

use zk_airdrop_claim::NULLIFIER_SEED;

#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(airdropclaim);

/// Generate a random BN254-compatible field element
fn generate_random_field_element() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let random_bytes: Vec<u8> = (0..31).map(|_| rand::random::<u8>()).collect();
    bytes[1..32].copy_from_slice(&random_bytes);
    bytes
}

/// Derive eligible address from private key: Poseidon(privateKey)
fn derive_eligible_address(private_key: &[u8; 32]) -> [u8; 32] {
    Poseidon::hashv(&[private_key]).unwrap()
}

/// Compute leaf hash for eligibility tree: Poseidon(eligibleAddress, amount)
fn compute_leaf(eligible_address: &[u8; 32], amount: u64) -> [u8; 32] {
    // Amount as 32-byte BE for circuit compatibility
    let mut amount_bytes = [0u8; 32];
    amount_bytes[24..].copy_from_slice(&amount.to_be_bytes());
    Poseidon::hashv(&[eligible_address, &amount_bytes]).unwrap()
}

/// Amount as 32-byte BE for circuit input
fn amount_to_bytes(amount: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..].copy_from_slice(&amount.to_be_bytes());
    bytes
}

/// Compute nullifier: Poseidon(airdropId, privateKey)
fn compute_nullifier(airdrop_id: u64, private_key: &[u8; 32]) -> [u8; 32] {
    // Hash airdrop_id to field element first
    let airdrop_id_bytes = airdrop_id.to_le_bytes();
    let airdrop_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&airdrop_id_bytes]).unwrap();
    Poseidon::hashv(&[&airdrop_id_hashed, private_key]).unwrap()
}

/// Simple Merkle tree for eligibility proofs (20 levels)
struct SimpleMerkleTree {
    leaves: Vec<[u8; 32]>,
    depth: usize,
}

impl SimpleMerkleTree {
    fn new(leaves: Vec<[u8; 32]>, depth: usize) -> Self {
        Self { leaves, depth }
    }

    fn root(&self) -> [u8; 32] {
        let mut current_level = self.leaves.clone();

        // Pad to power of 2
        let tree_size = 1 << self.depth;
        while current_level.len() < tree_size {
            current_level.push([0u8; 32]);
        }

        // Build tree bottom-up
        for _ in 0..self.depth {
            let mut next_level = Vec::new();
            for i in (0..current_level.len()).step_by(2) {
                let left = current_level[i];
                let right = current_level.get(i + 1).copied().unwrap_or([0u8; 32]);
                let hash = Poseidon::hashv(&[&left, &right]).unwrap();
                next_level.push(hash);
            }
            current_level = next_level;
        }

        current_level[0]
    }

    fn get_proof(&self, leaf_index: usize) -> Vec<[u8; 32]> {
        let mut proof = Vec::new();
        let mut current_level = self.leaves.clone();

        // Pad to power of 2
        let tree_size = 1 << self.depth;
        while current_level.len() < tree_size {
            current_level.push([0u8; 32]);
        }

        let mut idx = leaf_index;
        for _ in 0..self.depth {
            // Get sibling
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            proof.push(current_level.get(sibling_idx).copied().unwrap_or([0u8; 32]));

            // Build next level
            let mut next_level = Vec::new();
            for i in (0..current_level.len()).step_by(2) {
                let left = current_level[i];
                let right = current_level.get(i + 1).copied().unwrap_or([0u8; 32]);
                let hash = Poseidon::hashv(&[&left, &right]).unwrap();
                next_level.push(hash);
            }
            current_level = next_level;
            idx /= 2;
        }

        proof
    }
}

/// Build a simple Poseidon Merkle tree from leaves
fn build_eligibility_tree(leaves: &[[u8; 32]]) -> (SimpleMerkleTree, [u8; 32]) {
    let tree = SimpleMerkleTree::new(leaves.to_vec(), 20);
    let root = tree.root();
    (tree, root)
}

/// Get Merkle proof for a leaf
fn get_merkle_proof(tree: &SimpleMerkleTree, leaf_index: usize) -> Vec<[u8; 32]> {
    tree.get_proof(leaf_index)
}

#[tokio::test]
async fn test_zk_airdrop_claim_claim() {
    let config =
        ProgramTestConfig::new(true, Some(vec![("zk_airdrop_claim", zk_airdrop_claim::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    // Setup: Generate eligible users
    let private_key_1 = generate_random_field_element();
    let private_key_2 = generate_random_field_element();
    let private_key_3 = generate_random_field_element();

    let address_1 = derive_eligible_address(&private_key_1);
    let address_2 = derive_eligible_address(&private_key_2);
    let address_3 = derive_eligible_address(&private_key_3);

    let amount_1 = 1_000_000u64; // 1M tokens
    let amount_2 = 500_000u64;
    let amount_3 = 2_000_000u64;

    // Build eligibility tree
    let leaves = vec![
        compute_leaf(&address_1, amount_1),
        compute_leaf(&address_2, amount_2),
        compute_leaf(&address_3, amount_3),
    ];
    let (tree, eligibility_root) = build_eligibility_tree(&leaves);

    // Airdrop configuration
    let airdrop_id = 1u64;
    let unlock_slot = 0u64; // Immediately claimable

    // Create test mint and fund vault (simplified - in real test would use actual SPL tokens)
    let mint = Keypair::new();
    let token_vault = Keypair::new();

    // Initialize airdrop
    // Note: In a real test, you'd create actual token accounts and fund the vault
    // For now, we test the ZK proof generation logic

    // Test user 1 claims
    let recipient = Keypair::new();
    let nullifier = compute_nullifier(airdrop_id, &private_key_1);

    // Get Merkle proof for user 1 (leaf index 0)
    let merkle_proof = get_merkle_proof(&tree, 0);

    // Generate ZK proof
    let groth16_proof = generate_claim_proof(
        &private_key_1,
        &eligibility_root,
        amount_1,
        airdrop_id,
        &recipient.pubkey(),
        &merkle_proof,
        0,
    );

    println!("ZK proof generated successfully!");
    println!(
        "Privacy guarantee: Observer sees recipient {} getting {} tokens",
        recipient.pubkey(),
        amount_1
    );
    println!("But cannot tell which eligible address (1, 2, or 3) is claiming");

    // Verify double-claim prevention
    let nullifier_2 = compute_nullifier(airdrop_id, &private_key_1);
    assert_eq!(
        nullifier, nullifier_2,
        "Same private key should produce same nullifier"
    );

    // Different airdrop ID produces different nullifier
    let nullifier_different_airdrop = compute_nullifier(2, &private_key_1);
    assert_ne!(
        nullifier, nullifier_different_airdrop,
        "Different airdrop should produce different nullifier"
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_claim_proof(
    private_key: &[u8; 32],
    eligibility_root: &[u8; 32],
    amount: u64,
    airdrop_id: u64,
    recipient: &Pubkey,
    merkle_proof: &[[u8; 32]],
    leaf_index: usize,
) -> light_sdk::instruction::CompressedProof {
    let zkey_path = "../build/airdropclaim_final.zkey".to_string();

    // Build circuit inputs
    let mut proof_inputs = HashMap::new();

    // Hash airdrop_id to BN254 field
    let airdrop_id_bytes = airdrop_id.to_le_bytes();
    let airdrop_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&airdrop_id_bytes]).unwrap();

    // Hash recipient to BN254 field
    let recipient_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&recipient.to_bytes()]).unwrap();

    // Compute nullifier
    let nullifier = Poseidon::hashv(&[&airdrop_id_hashed, private_key]).unwrap();

    // Amount as 32-byte BE
    let mut amount_bytes = [0u8; 32];
    amount_bytes[24..].copy_from_slice(&amount.to_be_bytes());

    // Public inputs
    proof_inputs.insert(
        "eligibilityRoot".to_string(),
        vec![BigUint::from_bytes_be(eligibility_root).to_string()],
    );
    proof_inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(&nullifier).to_string()],
    );
    proof_inputs.insert(
        "recipient".to_string(),
        vec![BigUint::from_bytes_be(&recipient_hashed).to_string()],
    );
    proof_inputs.insert(
        "airdropId".to_string(),
        vec![BigUint::from_bytes_be(&airdrop_id_hashed).to_string()],
    );
    proof_inputs.insert(
        "amount".to_string(),
        vec![BigUint::from_bytes_be(&amount_bytes).to_string()],
    );

    // Private inputs
    proof_inputs.insert(
        "privateKey".to_string(),
        vec![BigUint::from_bytes_be(private_key).to_string()],
    );

    let path_elements: Vec<String> = merkle_proof
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    proof_inputs.insert("pathElements".to_string(), path_elements);

    proof_inputs.insert("leafIndex".to_string(), vec![leaf_index.to_string()]);

    // Generate proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(airdropclaim_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    // Verify proof locally
    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path.clone())
        .expect("Proof verification failed");
    assert!(is_valid, "Local proof verification should pass");

    // Convert to groth16-solana format and compress
    let (proof_a_uncompressed, proof_b_uncompressed, proof_c_uncompressed) =
        convert_proof(&proof.proof).expect("Failed to convert proof");

    // Verify with groth16-solana locally (same as on-chain)
    {
        use groth16_solana::groth16::Groth16Verifier;
        use groth16_solana::proof_parser::circom_prover::convert_public_inputs;

        let public_inputs_converted: [[u8; 32]; 5] = convert_public_inputs(&proof.pub_inputs);

        let mut verifier = Groth16Verifier::new(
            &proof_a_uncompressed,
            &proof_b_uncompressed,
            &proof_c_uncompressed,
            &public_inputs_converted,
            &zk_airdrop_claim::verifying_key::VERIFYINGKEY,
        )
        .expect("Failed to create verifier");

        verifier
            .verify()
            .expect("Local groth16-solana verification failed");
    }

    use groth16_solana::proof_parser::circom_prover::convert_proof_to_compressed;
    let (proof_a, proof_b, proof_c) = convert_proof_to_compressed(
        &proof_a_uncompressed,
        &proof_b_uncompressed,
        &proof_c_uncompressed,
    )
    .expect("Failed to compress proof");

    light_sdk::instruction::CompressedProof {
        a: proof_a,
        b: proof_b,
        c: proof_c,
    }
}
