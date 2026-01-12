use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::convert_proof;
use light_hasher::{hash_to_field_size::hash_to_bn254_field_size_be, Hasher, Poseidon};
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
use zk_merkle_proof::DATA_ACCOUNT;

#[link(name = "circuit", kind = "static")]
extern "C" {}

rust_witness::witness!(merkleproof);

#[tokio::test]
async fn test_create_and_verify_account() {
    let config =
        ProgramTestConfig::new(true, Some(vec![("zk_merkle_proof", zk_merkle_proof::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // First byte = 0 for BN254 field compatibility
    let mut secret_data = [0u8; 32];
    for i in 1..32 {
        secret_data[i] = (i as u8) + 65;
    }
    let data_hash = Poseidon::hashv(&[&secret_data]).unwrap();

    let address_tree_info = rpc.get_address_tree_v2();

    let (account_address, _) = derive_address(
        &[DATA_ACCOUNT, &data_hash],
        &address_tree_info.tree,
        &zk_merkle_proof::ID,
    );

    create_account(&mut rpc, &payer, &account_address, address_tree_info.clone(), data_hash)
        .await
        .unwrap();

    let accounts = rpc
        .get_compressed_accounts_by_owner(&zk_merkle_proof::ID, None, None)
        .await
        .unwrap();
    assert_eq!(accounts.value.items.len(), 1);
    let created_account = &accounts.value.items[0];

    let account_data_hash = created_account.data.as_ref().unwrap().data_hash;

    verify_account(&mut rpc, &payer, created_account, account_data_hash)
        .await
        .unwrap();
}

async fn create_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    data_hash: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(zk_merkle_proof::ID);
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

    let output_state_tree_index = rpc
        .get_random_state_tree_info_v1()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let instruction_data = zk_merkle_proof::instruction::CreateAccount {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        data_hash,
    };

    let accounts = zk_merkle_proof::accounts::CreateAccountAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: zk_merkle_proof::ID,
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

async fn verify_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    account: &light_client::indexer::CompressedAccount,
    data_hash: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let proofs_result = rpc
        .get_multiple_compressed_account_proofs(vec![account.hash], None)
        .await?;
    let proofs = proofs_result.value.items;

    assert!(!proofs.is_empty(), "No proofs returned");

    let merkle_proof = &proofs[0];
    let leaf_index = merkle_proof.leaf_index as u32;
    let merkle_proof_hashes = &merkle_proof.proof;
    let merkle_root = merkle_proof.root;
    let root_index = (merkle_proof.root_seq % 2400) as u16;
    let state_tree = merkle_proof.merkle_tree;

    let zk_proof = generate_merkle_proof(
        account,
        &state_tree,
        leaf_index,
        merkle_proof_hashes,
        &merkle_root,
        &data_hash,
    );

    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(zk_merkle_proof::ID);
    remaining_accounts.add_system_accounts(config)?;

    let instruction_data = zk_merkle_proof::instruction::VerifyAccount {
        input_root_index: root_index,
        zk_proof,
        data_hash,
    };

    let accounts = zk_merkle_proof::accounts::VerifyAccountAccounts {
        signer: payer.pubkey(),
        state_merkle_tree: state_tree,
    };

    let instruction = Instruction {
        program_id: zk_merkle_proof::ID,
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

fn generate_merkle_proof(
    account: &light_client::indexer::CompressedAccount,
    merkle_tree_pubkey: &Pubkey,
    leaf_index: u32,
    merkle_proof_hashes: &[[u8; 32]],
    merkle_root: &[u8; 32],
    data_hash: &[u8; 32],
) -> light_compressed_account::instruction_data::compressed_proof::CompressedProof {
    let zkey_path = "./build/merkle_proof_final.zkey".to_string();

    let mut proof_inputs = HashMap::new();

    let owner_hashed = hash_to_bn254_field_size_be(zk_merkle_proof::ID.as_ref());
    let merkle_tree_hashed = hash_to_bn254_field_size_be(merkle_tree_pubkey.as_ref());

    let discriminator = if let Some(ref data) = account.data {
        data.discriminator
    } else {
        [0u8; 8]
    };

    let address = account.address.expect("Account must have an address");

    // Verify hash can be recreated
    let mut leaf_index_bytes = [0u8; 32];
    leaf_index_bytes[28..32].copy_from_slice(&(account.leaf_index as u32).to_le_bytes());

    let mut discriminator_with_domain = [0u8; 32];
    discriminator_with_domain[24..32].copy_from_slice(&discriminator);
    discriminator_with_domain[23] = 2;

    let computed_hash = Poseidon::hashv(&[
        owner_hashed.as_slice(),
        leaf_index_bytes.as_slice(),
        merkle_tree_hashed.as_slice(),
        address.as_slice(),
        discriminator_with_domain.as_slice(),
        data_hash.as_slice(),
    ])
    .unwrap();

    assert_eq!(computed_hash, account.hash, "Hash mismatch");

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
        "data_hash".to_string(),
        vec![BigUint::from_bytes_be(data_hash).to_string()],
    );
    proof_inputs.insert(
        "expectedRoot".to_string(),
        vec![BigUint::from_bytes_be(merkle_root).to_string()],
    );

    // Private inputs
    proof_inputs.insert("leaf_index".to_string(), vec![leaf_index.to_string()]);

    let mut account_leaf_index_bytes = [0u8; 32];
    account_leaf_index_bytes[28..32].copy_from_slice(&(account.leaf_index as u32).to_le_bytes());
    proof_inputs.insert(
        "account_leaf_index".to_string(),
        vec![BigUint::from_bytes_be(&account_leaf_index_bytes).to_string()],
    );

    proof_inputs.insert(
        "address".to_string(),
        vec![BigUint::from_bytes_be(&address).to_string()],
    );

    let path_elements: Vec<String> = merkle_proof_hashes
        .iter()
        .map(|hash| BigUint::from_bytes_be(hash).to_string())
        .collect();
    proof_inputs.insert("pathElements".to_string(), path_elements);

    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();

    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(merkleproof_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path.clone())
        .expect("Proof verification failed");
    assert!(is_valid, "Local proof verification failed");

    // Verify with groth16-solana
    {
        use groth16_solana::groth16::Groth16Verifier;
        use groth16_solana::proof_parser::circom_prover::convert_public_inputs;

        let (proof_a, proof_b, proof_c) =
            convert_proof(&proof.proof).expect("Failed to convert proof");
        let public_inputs_converted: [[u8; 32]; 5] = convert_public_inputs(&proof.pub_inputs);

        let mut verifier = Groth16Verifier::new(
            &proof_a,
            &proof_b,
            &proof_c,
            &public_inputs_converted,
            &zk_merkle_proof::verifying_key::VERIFYINGKEY,
        )
        .expect("Failed to create verifier");

        verifier.verify().expect("groth16-solana verification failed");
    }

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
