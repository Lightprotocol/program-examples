// #![cfg(feature = "test-sbf")]

use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::convert_proof;
use light_client::indexer::CompressedAccount;
use light_hasher::{hash_to_field_size::hash_to_bn254_field_size_be, Hasher, Poseidon, Sha256};
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
use zk_id::{CREDENTIAL, ISSUER, ZK_ID_CHECK};

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

    /// Compute nullifier for a given verification_id
    pub fn compute_nullifier(&self, verification_id: &[u8; 31]) -> [u8; 32] {
        // Nullifier = Poseidon(verification_id, private_key)
        // Both need to be padded to 32 bytes for Poseidon
        let mut padded_verification = [0u8; 32];
        padded_verification[1..32].copy_from_slice(verification_id);

        Poseidon::hashv(&[&padded_verification, &self.private_key]).unwrap()
    }
}

// Link the generated witness library
#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(compressedaccountmerkleproof);

#[tokio::test]
async fn test_create_issuer_and_add_credential() {
    let config = ProgramTestConfig::new(true, Some(vec![("zk_id", zk_id::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    let (issuer_address, _) = derive_address(
        &[ISSUER, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &zk_id::ID,
    );
    println!("issuer_address {:?}", issuer_address);
    // Step 1: Create the issuer account
    create_issuer(&mut rpc, &payer, &issuer_address, address_tree_info.clone())
        .await
        .unwrap();

    // Verify the issuer account was created
    let issuer_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_id::ID, None, None)
        .await
        .unwrap();
    assert_eq!(issuer_accounts.value.items.len(), 1);
    let issuer_account = &issuer_accounts.value.items[0];

    println!("Created issuer account for pubkey: {}", payer.pubkey());

    // Step 2: Create a credential account
    // Create a credential keypair for the user
    let user_keypair = Keypair::new();
    let credential = CredentialKeypair::new(&user_keypair);

    // Use the credential commitment as the "pubkey" for address derivation
    let (credential_address, _) = derive_address(
        &[CREDENTIAL, credential.public_key.as_ref()],
        &address_tree_info.tree,
        &zk_id::ID,
    );

    add_credential(
        &mut rpc,
        &payer,
        &credential_address,
        address_tree_info.clone(),
        issuer_account,
        credential.public_key,
    )
    .await
    .unwrap();

    // Verify both accounts exist now (issuer + credential)
    let program_compressed_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_id::ID, None, None)
        .await
        .unwrap();

    assert_eq!(program_compressed_accounts.value.items.len(), 2);
    println!(
        "program_compressed_accounts.value.items {:?}",
        program_compressed_accounts.value.items
    );

    println!(
        "Successfully created credential account with public_key: {:?}",
        credential.public_key
    );

    // Step 3: Verify the credential with ZK proof
    let credential_account = rpc
        .get_compressed_account(credential_address, None)
        .await
        .unwrap()
        .value
        .expect("Credential account not found");
    println!("credential_account {:?}", credential_account);
    verify_credential(
        &mut rpc,
        &payer,
        &credential_account,
        address_tree_info,
        &user_keypair,
    )
    .await
    .unwrap();

    println!("Successfully verified credential with ZK proof!");

    // Verify event account was created
    let final_compressed_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_id::ID, None, None)
        .await
        .unwrap();

    assert_eq!(final_compressed_accounts.value.items.len(), 3);
}

async fn create_issuer<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_pre_accounts_signer(payer.pubkey());
    let config = SystemAccountMetaConfig::new(zk_id::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

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
    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let (remaining_accounts_metas, system_accounts_offset, _) = remaining_accounts.to_account_metas();

    let instruction_data = zk_id::instruction::CreateIssuer {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        system_accounts_offset: system_accounts_offset as u8,
    };

    let accounts = zk_id::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: zk_id::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn add_credential<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    issuer_account: &CompressedAccount,
    credential_commitment: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_pre_accounts_signer(payer.pubkey());
    let config = SystemAccountMetaConfig::new(zk_id::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![issuer_account.hash],
            vec![AddressWithTree {
                address: *address,
                tree: address_tree_info.tree,
            }],
            None,
        )
        .await?
        .value;

    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();
    let packed_address_tree_accounts = packed_tree_accounts.address_trees;

    // Create the issuer account meta manually
    let issuer_account_meta = light_sdk::instruction::account_meta::CompressedAccountMeta {
        tree_info: packed_state_tree_accounts.packed_tree_infos[0],
        address: issuer_account.address.unwrap(),
        output_state_tree_index: packed_state_tree_accounts.output_tree_index,
    };

    let output_state_tree_index = rpc
        .get_random_state_tree_info_v1()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    // Parse the issuer account data to get num_credentials_issued
    let issuer_data = issuer_account.data.as_ref().unwrap();
    let issuer_account_parsed: zk_id::IssuerAccount =
        anchor_lang::AnchorDeserialize::deserialize(&mut issuer_data.data.as_slice()).unwrap();

    let (remaining_accounts_metas, system_accounts_offset, _) = remaining_accounts.to_account_metas();

    let instruction_data = zk_id::instruction::AddCredential {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        system_accounts_offset: system_accounts_offset as u8,
        issuer_account_meta,
        credential_pubkey: Pubkey::new_from_array(credential_commitment),
        num_credentials_issued: issuer_account_parsed.num_credentials_issued,
    };

    let accounts = zk_id::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: zk_id::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn verify_credential<R>(
    rpc: &mut R,
    payer: &Keypair,
    credential_account: &CompressedAccount,
    address_tree_info: light_client::indexer::TreeInfo,
    user_keypair: &Keypair,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Get the merkle proof for the credential account
    let proofs = rpc
        .get_multiple_compressed_account_proofs(vec![credential_account.hash], None)
        .await?
        .value
        .items;

    let merkle_proof = &proofs[0];
    let leaf_index = merkle_proof.leaf_index as u32;
    let merkle_proof_hashes = &merkle_proof.proof;
    let merkle_root = merkle_proof.root;
    let root_index = (merkle_proof.root_seq % 2400) as u16;

    let state_tree = merkle_proof.merkle_tree;

    // Parse the credential account data
    let credential_data = credential_account.data.as_ref().unwrap();
    let credential_account_parsed: zk_id::CredentialAccount =
        anchor_lang::AnchorDeserialize::deserialize(&mut credential_data.data.as_slice()).unwrap();

    // Generate encrypted data (in a real scenario, this would be user-provided)
    let encrypted_data = vec![42u8; 64];

    // Create the credential keypair from the user keypair
    let credential = CredentialKeypair::new(user_keypair);

    // Generate a verification_id (31 bytes)
    let mut verification_id = [0u8; 31];
    let random_pubkey = Pubkey::new_unique();
    verification_id.copy_from_slice(&random_pubkey.to_bytes()[0..31]);

    // Generate the ZK proof using the actual merkle root
    let (credential_proof, nullifier) = generate_credential_proof(
        credential_account,
        &state_tree,
        leaf_index,
        &merkle_proof_hashes,
        &merkle_root,
        &credential_account_parsed.issuer,
        &credential,
        &encrypted_data,
        &verification_id,
    );

    // Create the verification transaction
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_pre_accounts_signer(payer.pubkey());
    let config = SystemAccountMetaConfig::new(zk_id::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let (event_address, _) = derive_address(
        &[
            ZK_ID_CHECK,
            nullifier.as_slice(),
            verification_id.as_slice(),
        ],
        &address_tree_info.tree,
        &zk_id::ID,
    );

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: event_address,
                tree: address_tree_info.tree,
            }],
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

    let (remaining_accounts_metas, system_accounts_offset, _) = remaining_accounts.to_account_metas();

    let instruction_data = zk_id::instruction::ZkVerifyCredential {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        system_accounts_offset: system_accounts_offset as u8,
        input_root_index: root_index,
        public_data: encrypted_data,
        credential_proof,
        issuer: credential_account_parsed.issuer.to_bytes(),
        nullifier,
        verification_id,
    };

    let accounts = zk_id::accounts::VerifyAccounts {
        signer: payer.pubkey(),
        input_merkle_tree: state_tree,
    };

    let instruction = Instruction {
        program_id: zk_id::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

fn generate_credential_proof(
    credential_account: &CompressedAccount,
    merkle_tree_pubkey: &Pubkey,
    leaf_index: u32,
    merkle_proof_hashes: &[[u8; 32]],
    merkle_root: &[u8; 32],
    issuer_pubkey: &Pubkey,
    credential: &CredentialKeypair,
    encrypted_data: &[u8],
    verification_id: &[u8; 31],
) -> (
    light_compressed_account::instruction_data::compressed_proof::CompressedProof,
    [u8; 32], // nullifier
) {
    let zkey_path = "./build/compressed_account_merkle_proof_final.zkey".to_string();

    // Build circuit inputs
    let mut proof_inputs = HashMap::new();

    // Add compressed account inputs
    let discriminator = if let Some(ref data) = credential_account.data {
        data.discriminator
    } else {
        [0u8; 8]
    };

    let owner_hashed = hash_to_bn254_field_size_be(zk_id::ID.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());

    // Use the same hashing as on-chain: hashv_to_bn254_field_size_be_const_array::<2>
    use light_hasher::hash_to_field_size::hashv_to_bn254_field_size_be_const_array;
    let issuer_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[issuer_pubkey.as_ref()]).unwrap();

    // Compute data_hash as hash of issuer and credential commitment (public key is already a Poseidon hash)
    let mut hash_input = Vec::new();
    hash_input.extend_from_slice((encrypted_data.len() as u32).to_le_bytes().as_ref());
    hash_input.extend_from_slice(encrypted_data);
    let mut encrypted_data_hash = Sha256::hash(hash_input.as_slice()).unwrap();
    encrypted_data_hash[0] = 0;

    let public_data_hash =
        Poseidon::hashv(&[issuer_hashed.as_slice(), &credential.public_key]).unwrap();

    // Verify the data_hash matches
    let expected_data_hash = credential_account.data.as_ref().unwrap().data_hash;
    assert_eq!(public_data_hash, expected_data_hash, "Data hash mismatch");

    // Compute what the circuit will compute for the leaf hash
    // The circuit adds 36893488147419103232 (0x2000000000000000) to discriminator
    // This effectively puts a 2 prefix at byte 23 (counting from right in BE)

    // SDK format: 32-byte array with leaf_index in LE at [28..32]
    let mut leaf_index_bytes = [0u8; 32];
    leaf_index_bytes[28..32].copy_from_slice(&(leaf_index as u32).to_le_bytes());

    // SDK format: 32-byte array with discriminator at [24..32] and prefix 2 at [23]
    let mut discriminator_bytes = [0u8; 32];
    discriminator_bytes[24..32].copy_from_slice(&discriminator);
    discriminator_bytes[23] = 2;

    let computed_leaf_hash = Poseidon::hashv(&[
        owner_hashed.as_slice(),
        leaf_index_bytes.as_slice(),
        merkle_tree_hashed.as_slice(),
        credential_account.address.as_ref().unwrap().as_ref(),
        discriminator_bytes.as_slice(),
        public_data_hash.as_slice(),
    ])
    .unwrap();

    assert_eq!(
        computed_leaf_hash, credential_account.hash,
        "Leaf hash mismatch - circuit cannot recreate account hash"
    );

    proof_inputs.insert(
        "owner_hashed".to_string(),
        vec![BigUint::from_bytes_be(&owner_hashed).to_string()],
    );
    proof_inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    // account_leaf_index needs to be in the same format as SDK: 32-byte array with value at [28..32] in LE
    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32]
        .copy_from_slice(&(credential_account.leaf_index as u32).to_le_bytes());
    proof_inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    // Add address field - credential account has an address
    let address = credential_account.address.unwrap_or([0u8; 32]);
    proof_inputs.insert(
        "address".to_string(),
        vec![BigUint::from_bytes_be(&address).to_string()],
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
        "issuer_hashed".to_string(),
        vec![BigUint::from_bytes_be(&issuer_hashed).to_string()],
    );

    // Add credential private key (private input) - already padded to 32 bytes
    proof_inputs.insert(
        "credentialPrivateKey".to_string(),
        vec![BigUint::from_bytes_be(&credential.private_key).to_string()],
    );

    proof_inputs.insert(
        "encrypted_data_hash".to_string(),
        vec![BigUint::from_bytes_be(&encrypted_data_hash).to_string()],
    );
    proof_inputs.insert(
        "public_encrypted_data_hash".to_string(),
        vec![BigUint::from_bytes_be(&encrypted_data_hash).to_string()],
    );

    // Add verification_id (public input) - pad to 32 bytes
    let mut padded_verification = [0u8; 32];
    padded_verification[1..32].copy_from_slice(verification_id);
    proof_inputs.insert(
        "verification_id".to_string(),
        vec![BigUint::from_bytes_be(&padded_verification).to_string()],
    );

    // Compute nullifier
    let nullifier = credential.compute_nullifier(verification_id);
    proof_inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(&nullifier).to_string()],
    );

    // Add merkle proof inputs
    let path_elements: Vec<String> = merkle_proof_hashes
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    proof_inputs.insert("pathElements".to_string(), path_elements);

    // Use the actual merkle root from the indexer
    let expected_root_bigint = BigUint::from_bytes_be(merkle_root);
    proof_inputs.insert(
        "expectedRoot".to_string(),
        vec![expected_root_bigint.to_string()],
    );

    // Generate proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(compressedaccountmerkleproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    // Verify proof locally
    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path.clone())
        .expect("Proof verification failed");
    assert!(is_valid, "Local circom proof verification should pass");

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

    // Verify with groth16-solana locally (same as on-chain)
    {
        use groth16_solana::groth16::Groth16Verifier;
        use groth16_solana::proof_parser::circom_prover::convert_public_inputs;

        // Convert public inputs from the circom proof (8 public inputs in circuit)
        let public_inputs_converted: [[u8; 32]; 8] = convert_public_inputs(&proof.pub_inputs);
        println!("public_inputs_converted {:?}", public_inputs_converted);
        // Create verifier using the uncompressed proofs (which have proof_a negated)
        let mut verifier = Groth16Verifier::new(
            &proof_a_uncompressed,
            &proof_b_uncompressed,
            &proof_c_uncompressed,
            &public_inputs_converted,
            &zk_id::verifying_key::VERIFYINGKEY,
        )
        .expect("Failed to create verifier");

        // Verify
        verifier
            .verify()
            .expect("Local groth16-solana verification failed");
    }

    let compressed_proof =
        light_compressed_account::instruction_data::compressed_proof::CompressedProof {
            a: proof_a,
            b: proof_b,
            c: proof_c,
        };

    (compressed_proof, nullifier)
}
