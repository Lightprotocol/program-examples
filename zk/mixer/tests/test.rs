// #![cfg(feature = "test-sbf")]

use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::convert_proof;
use light_hasher::{
    hash_to_field_size::{hash_to_bn254_field_size_be, hashv_to_bn254_field_size_be_const_array},
    Hasher, Poseidon,
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

use mixer::{COMMITMENT, NULLIFIER};

#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(withdraw);

fn generate_random_field_element() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let random_bytes: Vec<u8> = (0..31).map(|_| rand::random::<u8>()).collect();
    bytes[1..32].copy_from_slice(&random_bytes);
    bytes
}

fn compute_commitment(nullifier: &[u8; 32], secret: &[u8; 32]) -> [u8; 32] {
    Poseidon::hashv(&[nullifier, secret]).unwrap()
}

fn compute_nullifier_hash(nullifier: &[u8; 32]) -> [u8; 32] {
    Poseidon::hashv(&[nullifier]).unwrap()
}

#[tokio::test]
async fn test_mixer_deposit_and_withdraw() {
    let config = ProgramTestConfig::new(true, Some(vec![("mixer", mixer::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    let mixer_config = Keypair::new();
    let denomination = 1_000_000_000u64; // 1 SOL

    initialize_mixer(&mut rpc, &payer, &mixer_config, denomination)
        .await
        .unwrap();

    // Generate deposit secrets
    let nullifier = generate_random_field_element();
    let secret = generate_random_field_element();
    let commitment = compute_commitment(&nullifier, &secret);

    // Derive commitment address
    let (commitment_address, _) = derive_address(
        &[COMMITMENT, commitment.as_slice()],
        &address_tree_info.tree,
        &mixer::ID,
    );

    // Fund the depositor
    let depositor = Keypair::new();
    fund_account(&mut rpc, &payer, &depositor.pubkey(), denomination * 2)
        .await
        .unwrap();

    // Deposit
    deposit(
        &mut rpc,
        &depositor,
        &mixer_config.pubkey(),
        &commitment_address,
        address_tree_info.clone(),
        commitment,
    )
    .await
    .unwrap();

    // Verify commitment account was created
    let program_accounts = rpc
        .get_compressed_accounts_by_owner(&mixer::ID, None, None)
        .await
        .unwrap();
    assert_eq!(program_accounts.value.items.len(), 1);

    // Get the commitment account for merkle proof
    let commitment_account = rpc
        .get_compressed_account(commitment_address, None)
        .await
        .unwrap()
        .value
        .expect("Commitment account not found");

    // Get Merkle proof for the commitment account
    let proofs = rpc
        .get_multiple_compressed_account_proofs(vec![commitment_account.hash], None)
        .await
        .unwrap()
        .value
        .items;

    let merkle_proof = &proofs[0];
    let merkle_root = merkle_proof.root;
    let path_elements = &merkle_proof.proof;
    let leaf_index = merkle_proof.leaf_index as u32;
    let root_index = (merkle_proof.root_seq % 2400) as u16;
    let state_tree = merkle_proof.merkle_tree;

    // Generate ZK proof
    let recipient = Keypair::new();
    let nullifier_hash = compute_nullifier_hash(&nullifier);

    let groth16_proof = generate_withdraw_proof(
        &commitment_account,
        &state_tree,
        leaf_index,
        path_elements,
        &merkle_root,
        &nullifier,
        &secret,
        &recipient.pubkey(),
    );

    // Derive nullifier address
    let (nullifier_address, _) = derive_address(
        &[NULLIFIER, nullifier_hash.as_slice()],
        &address_tree_info.tree,
        &mixer::ID,
    );

    // Withdraw
    withdraw(
        &mut rpc,
        &payer,
        &mixer_config.pubkey(),
        &recipient.pubkey(),
        &nullifier_address,
        address_tree_info,
        &state_tree,
        root_index,
        groth16_proof,
        mixer::WithdrawPublicInputs {
            nullifier_hash,
            recipient: recipient.pubkey().to_bytes(),
        },
    )
    .await
    .unwrap();

    // Verify nullifier account was created (prevents double-spend)
    let final_accounts = rpc
        .get_compressed_accounts_by_owner(&mixer::ID, None, None)
        .await
        .unwrap();
    assert_eq!(final_accounts.value.items.len(), 2);
}

async fn initialize_mixer<R>(
    rpc: &mut R,
    payer: &Keypair,
    mixer_config: &Keypair,
    denomination: u64,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let instruction_data = mixer::instruction::Initialize { denomination };

    let accounts = mixer::accounts::Initialize {
        mixer_config: mixer_config.pubkey(),
        authority: payer.pubkey(),
        system_program: solana_sdk::system_program::ID,
    };

    let instruction = Instruction {
        program_id: mixer::ID,
        accounts: accounts.to_account_metas(None),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer, mixer_config])
        .await
}

async fn fund_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    account: &Pubkey,
    lamports: u64,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let instruction = system_instruction::transfer(&payer.pubkey(), account, lamports);
    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

async fn deposit<R>(
    rpc: &mut R,
    depositor: &Keypair,
    mixer_config: &Pubkey,
    commitment_address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    commitment: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(mixer::ID);
    remaining_accounts.add_system_accounts(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: *commitment_address,
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

    // Derive vault PDA
    let (vault, _) = Pubkey::find_program_address(&[b"vault", mixer_config.as_ref()], &mixer::ID);

    let instruction_data = mixer::instruction::Deposit {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        commitment,
    };

    let accounts = mixer::accounts::Deposit {
        mixer_config: *mixer_config,
        vault,
        depositor: depositor.pubkey(),
        system_program: solana_sdk::system_program::ID,
    };

    let instruction = Instruction {
        program_id: mixer::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &depositor.pubkey(), &[depositor])
        .await
}

#[allow(clippy::too_many_arguments)]
async fn withdraw<R>(
    rpc: &mut R,
    payer: &Keypair,
    mixer_config: &Pubkey,
    recipient: &Pubkey,
    nullifier_address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    input_merkle_tree: &Pubkey,
    input_root_index: u16,
    groth16_proof: light_sdk::instruction::CompressedProof,
    public_inputs: mixer::WithdrawPublicInputs,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(mixer::ID);
    remaining_accounts.add_system_accounts(config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address: *nullifier_address,
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

    // Derive vault PDA
    let (vault, _) = Pubkey::find_program_address(&[b"vault", mixer_config.as_ref()], &mixer::ID);

    let instruction_data = mixer::instruction::Withdraw {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        input_root_index,
        groth16_proof,
        public_inputs,
    };

    let accounts = mixer::accounts::Withdraw {
        mixer_config: *mixer_config,
        vault,
        recipient: *recipient,
        payer: payer.pubkey(),
        input_merkle_tree: *input_merkle_tree,
        system_program: solana_sdk::system_program::ID,
    };

    let instruction = Instruction {
        program_id: mixer::ID,
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

#[allow(clippy::too_many_arguments)]
fn generate_withdraw_proof(
    commitment_account: &light_client::indexer::CompressedAccount,
    merkle_tree_pubkey: &Pubkey,
    leaf_index: u32,
    merkle_proof_hashes: &[[u8; 32]],
    merkle_root: &[u8; 32],
    nullifier: &[u8; 32],
    secret: &[u8; 32],
    recipient: &Pubkey,
) -> light_sdk::instruction::CompressedProof {
    let zkey_path = "./build/withdraw_final.zkey".to_string();

    // Build circuit inputs
    let mut proof_inputs = HashMap::new();

    // Compute hashed values matching on-chain computation
    let owner_hashed = hash_to_bn254_field_size_be(mixer::ID.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());

    // Get discriminator from commitment account
    let discriminator = if let Some(ref data) = commitment_account.data {
        data.discriminator
    } else {
        [0u8; 8]
    };

    // Compute commitment = Poseidon(nullifier, secret)
    let commitment = Poseidon::hashv(&[nullifier, secret]).unwrap();

    // LightHasher adds a Poseidon layer: data_hash = Poseidon(commitment)
    let data_hash = Poseidon::hashv(&[&commitment]).unwrap();

    // Verify our computed data_hash matches the account's data_hash
    if let Some(ref data) = commitment_account.data {
        assert_eq!(
            data_hash, data.data_hash,
            "Computed data_hash doesn't match stored data_hash"
        );
    }

    // SDK format: 32-byte array with leaf_index in LE at [28..32]
    let mut leaf_index_bytes = [0u8; 32];
    leaf_index_bytes[28..32].copy_from_slice(&(leaf_index as u32).to_le_bytes());

    // SDK format: 32-byte array with discriminator at [24..32] and prefix 2 at [23]
    let mut discriminator_bytes = [0u8; 32];
    discriminator_bytes[24..32].copy_from_slice(&discriminator);
    discriminator_bytes[23] = 2;

    // Verify our computed leaf hash matches the account hash
    let computed_leaf_hash = Poseidon::hashv(&[
        owner_hashed.as_slice(),
        leaf_index_bytes.as_slice(),
        merkle_tree_hashed.as_slice(),
        commitment_account.address.as_ref().unwrap().as_ref(),
        discriminator_bytes.as_slice(),
        data_hash.as_slice(),
    ])
    .unwrap();

    assert_eq!(
        computed_leaf_hash, commitment_account.hash,
        "Leaf hash mismatch - circuit cannot recreate account hash"
    );

    // Discriminator is padded to 32 bytes (without the domain prefix - circuit adds it)
    let mut discriminator_for_circuit = [0u8; 32];
    discriminator_for_circuit[24..].copy_from_slice(&discriminator);

    let nullifier_hash = Poseidon::hashv(&[nullifier]).unwrap();

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
        vec![BigUint::from_bytes_be(&discriminator_for_circuit).to_string()],
    );
    proof_inputs.insert(
        "expectedRoot".to_string(),
        vec![BigUint::from_bytes_be(merkle_root).to_string()],
    );
    proof_inputs.insert(
        "nullifierHash".to_string(),
        vec![BigUint::from_bytes_be(&nullifier_hash).to_string()],
    );
    // Hash recipient to fit in BN254 field (pubkeys are 256 bits, field is ~254 bits)
    let recipient_hashed =
        hashv_to_bn254_field_size_be_const_array::<2>(&[&recipient.to_bytes()]).unwrap();
    proof_inputs.insert(
        "recipient_hashed".to_string(),
        vec![BigUint::from_bytes_be(&recipient_hashed).to_string()],
    );

    // Private inputs
    proof_inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(nullifier).to_string()],
    );
    proof_inputs.insert(
        "secret".to_string(),
        vec![BigUint::from_bytes_be(secret).to_string()],
    );

    // leaf_index - raw index for Merkle path calculation (0, 1, 2, ...)
    proof_inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    // account_leaf_index - SDK-formatted 32-byte encoding for account hash
    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32]
        .copy_from_slice(&(commitment_account.leaf_index as u32).to_le_bytes());
    proof_inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    // Address
    let address = commitment_account.address.unwrap_or([0u8; 32]);
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
        WitnessFn::RustWitness(withdraw_witness),
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

        let public_inputs_converted: [[u8; 32]; 6] = convert_public_inputs(&proof.pub_inputs);

        let mut verifier = Groth16Verifier::new(
            &proof_a_uncompressed,
            &proof_b_uncompressed,
            &proof_c_uncompressed,
            &public_inputs_converted,
            &mixer::verifying_key::VERIFYINGKEY,
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
