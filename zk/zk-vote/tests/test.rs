use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::convert_proof;
use light_client::indexer::CompressedAccount;
use light_hasher::{
    hash_to_field_size::{hash_to_bn254_field_size_be, hashv_to_bn254_field_size_be_const_array},
    Hasher, Poseidon, Sha256,
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
    system_program,
};
use std::collections::HashMap;
use zk_vote::{POLL, VOTER, VOTE_RECORD};

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
        let message = b"CREDENTIAL";
        let signature = solana_keypair.sign_message(message);
        let hashed = Sha256::hash(signature.as_ref()).unwrap();

        let mut private_key = [0u8; 32];
        private_key[1..32].copy_from_slice(&hashed[0..31]);

        let public_key = Poseidon::hashv(&[&private_key]).unwrap();

        Self {
            private_key,
            public_key,
        }
    }

    /// Compute nullifier for a given poll_id_hashed
    pub fn compute_nullifier(&self, poll_id_hashed: &[u8; 32]) -> [u8; 32] {
        Poseidon::hashv(&[poll_id_hashed, &self.private_key]).unwrap()
    }
}

// Link the generated witness library
#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(voteproof);

/// Derive Poll PDA address
fn derive_poll_address(authority: &Pubkey, poll_id: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[POLL, authority.as_ref(), &poll_id.to_le_bytes()],
        &zk_vote::ID,
    )
}

#[tokio::test]
async fn test_create_poll_and_vote() {
    let config = ProgramTestConfig::new(true, Some(vec![("zk_vote", zk_vote::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let poll_id: u32 = 1;

    // Step 1: Create a poll
    let (poll_address, _) = derive_poll_address(&payer.pubkey(), poll_id);
    create_poll(
        &mut rpc,
        &payer,
        poll_id,
        "What's your favorite color?".to_string(),
        "Red".to_string(),
        "Green".to_string(),
        "Blue".to_string(),
    )
    .await
    .unwrap();

    println!("Created poll at address: {}", poll_address);

    // Step 2: Register a voter
    let address_tree_info = rpc.get_address_tree_v2();
    let user_keypair = Keypair::new();
    let credential = CredentialKeypair::new(&user_keypair);

    let (credential_address, _) = derive_address(
        &[VOTER, &poll_id.to_le_bytes(), &credential.public_key],
        &address_tree_info.tree,
        &zk_vote::ID,
    );

    register_voter(
        &mut rpc,
        &payer,
        poll_id,
        address_tree_info.clone(),
        credential.public_key,
    )
    .await
    .unwrap();

    println!(
        "Registered voter with credential pubkey: {:?}",
        credential.public_key
    );

    // Fund the voter so they can pay for the vote transaction
    rpc.airdrop_lamports(&user_keypair.pubkey(), 1_000_000_000)
        .await
        .unwrap();

    // Verify the credential was created and get it from the list
    let program_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_vote::ID, None, None)
        .await
        .unwrap();
    assert_eq!(program_accounts.value.items.len(), 1);

    // Step 3: Cast a vote with ZK proof
    // Use the account from the owner query which has the correct hash
    let credential_account = &program_accounts.value.items[0];
    println!(
        "credential_account.hash: {:?}, address: {:?}",
        credential_account.hash, credential_account.address
    );

    let vote_choice = 2u8; // Vote for "Blue"

    vote_with_proof(
        &mut rpc,
        &user_keypair,
        credential_account,
        poll_id,
        vote_choice,
        address_tree_info.clone(),
        &credential,
    )
    .await
    .unwrap();

    println!("Successfully cast vote for option {}", vote_choice);

    // Verify vote record was created
    let final_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_vote::ID, None, None)
        .await
        .unwrap();
    assert_eq!(final_accounts.value.items.len(), 2); // credential + vote record

    // Step 4: Close the poll
    close_poll(&mut rpc, &payer, poll_id).await.unwrap();
    println!("Poll closed successfully!");
}

async fn create_poll<R>(
    rpc: &mut R,
    payer: &Keypair,
    poll_id: u32,
    question: String,
    option_0: String,
    option_1: String,
    option_2: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let (poll_address, _) = derive_poll_address(&payer.pubkey(), poll_id);

    let instruction_data = zk_vote::instruction::CreatePoll {
        poll_id,
        question,
        option_0,
        option_1,
        option_2,
    };

    let accounts = zk_vote::accounts::CreatePollAccounts {
        authority: payer.pubkey(),
        poll: poll_address,
        system_program: system_program::ID,
    };

    let instruction = Instruction {
        program_id: zk_vote::ID,
        accounts: accounts.to_account_metas(None),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn register_voter<R>(
    rpc: &mut R,
    payer: &Keypair,
    poll_id: u32,
    address_tree_info: light_client::indexer::TreeInfo,
    credential_pubkey: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(zk_vote::ID);
    remaining_accounts.add_system_accounts(config)?;

    let (credential_address, _) = derive_address(
        &[VOTER, &poll_id.to_le_bytes(), &credential_pubkey],
        &address_tree_info.tree,
        &zk_vote::ID,
    );

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: credential_address,
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

    let (poll_address, _) = derive_poll_address(&payer.pubkey(), poll_id);

    let instruction_data = zk_vote::instruction::RegisterVoter {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        credential_pubkey,
    };

    let accounts = zk_vote::accounts::RegisterVoterAccounts {
        authority: payer.pubkey(),
        poll: poll_address,
    };

    let instruction = Instruction {
        program_id: zk_vote::ID,
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

async fn vote_with_proof<R>(
    rpc: &mut R,
    voter: &Keypair,
    credential_account: &CompressedAccount,
    poll_id: u32,
    vote_choice: u8,
    address_tree_info: light_client::indexer::TreeInfo,
    credential: &CredentialKeypair,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    // Get Merkle proof for the credential
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

    // Generate ZK proof
    let poll_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&poll_id.to_le_bytes()]).unwrap();
    let nullifier = credential.compute_nullifier(&poll_id_hashed);

    let (credential_proof, _) = generate_vote_proof(
        credential_account,
        &state_tree,
        leaf_index,
        merkle_proof_hashes,
        &merkle_root,
        poll_id,
        vote_choice,
        credential,
    );

    // Build transaction
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(zk_vote::ID);
    remaining_accounts.add_system_accounts(config)?;

    let (vote_record_address, _) = derive_address(
        &[VOTE_RECORD, &nullifier, &poll_id.to_le_bytes()],
        &address_tree_info.tree,
        &zk_vote::ID,
    );

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: vote_record_address,
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

    // Get poll authority to derive poll address
    // For this test, we use the payer from the test context
    // In production, you'd get this from the poll account or pass it explicitly
    let poll_authority = rpc.get_payer().pubkey();
    let (poll_address, _) = derive_poll_address(&poll_authority, poll_id);

    let instruction_data = zk_vote::instruction::Vote {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        input_root_index: root_index,
        vote_choice,
        credential_proof,
        nullifier,
    };

    let accounts = zk_vote::accounts::VoteAccounts {
        voter: voter.pubkey(),
        poll: poll_address,
        input_merkle_tree: state_tree,
    };

    let instruction = Instruction {
        program_id: zk_vote::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &voter.pubkey(), &[voter])
        .await
}

fn generate_vote_proof(
    credential_account: &CompressedAccount,
    merkle_tree_pubkey: &Pubkey,
    leaf_index: u32,
    merkle_proof_hashes: &[[u8; 32]],
    merkle_root: &[u8; 32],
    poll_id: u32,
    vote_choice: u8,
    credential: &CredentialKeypair,
) -> (
    light_compressed_account::instruction_data::compressed_proof::CompressedProof,
    [u8; 32], // nullifier
) {
    let zkey_path = "./build/vote_proof_final.zkey".to_string();

    let mut proof_inputs = HashMap::new();

    // Hash values
    let owner_hashed = hash_to_bn254_field_size_be(zk_vote::ID.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());
    let poll_id_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&poll_id.to_le_bytes()]).unwrap();

    let discriminator = if let Some(ref data) = credential_account.data {
        data.discriminator
    } else {
        [0u8; 8]
    };

    let nullifier = credential.compute_nullifier(&poll_id_hashed);

    // Public inputs
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
        "poll_id_hashed".to_string(),
        vec![BigUint::from_bytes_be(&poll_id_hashed).to_string()],
    );
    proof_inputs.insert(
        "expectedRoot".to_string(),
        vec![BigUint::from_bytes_be(merkle_root).to_string()],
    );
    proof_inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(&nullifier).to_string()],
    );
    proof_inputs.insert("vote_choice".to_string(), vec![vote_choice.to_string()]);

    // Private inputs
    proof_inputs.insert(
        "credentialPrivateKey".to_string(),
        vec![BigUint::from_bytes_be(&credential.private_key).to_string()],
    );
    proof_inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32]
        .copy_from_slice(&(credential_account.leaf_index as u32).to_le_bytes());
    proof_inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    let address = credential_account.address.unwrap_or([0u8; 32]);
    proof_inputs.insert(
        "address".to_string(),
        vec![BigUint::from_bytes_be(&address).to_string()],
    );

    let path_elements: Vec<String> = merkle_proof_hashes
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    proof_inputs.insert("pathElements".to_string(), path_elements);

    // Generate proof
    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(voteproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    // Verify locally
    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path.clone())
        .expect("Proof verification failed");
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

    // Verify with groth16-solana locally
    {
        use groth16_solana::groth16::Groth16Verifier;
        use groth16_solana::proof_parser::circom_prover::convert_public_inputs;

        let public_inputs_converted: [[u8; 32]; 7] = convert_public_inputs(&proof.pub_inputs);

        let mut verifier = Groth16Verifier::new(
            &proof_a_uncompressed,
            &proof_b_uncompressed,
            &proof_c_uncompressed,
            &public_inputs_converted,
            &zk_vote::verifying_key::VERIFYINGKEY,
        )
        .expect("Failed to create verifier");

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

async fn close_poll<R>(rpc: &mut R, payer: &Keypair, poll_id: u32) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let (poll_address, _) = derive_poll_address(&payer.pubkey(), poll_id);

    let instruction_data = zk_vote::instruction::ClosePoll {};

    let accounts = zk_vote::accounts::ClosePollAccounts {
        authority: payer.pubkey(),
        poll: poll_address,
    };

    let instruction = Instruction {
        program_id: zk_vote::ID,
        accounts: accounts.to_account_metas(None),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
