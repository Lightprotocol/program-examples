use anchor_lang::{InstructionData, ToAccountMetas};
use circom_prover::{prover::ProofLib, witness::WitnessFn, CircomProver};
use groth16_solana::proof_parser::circom_prover::{convert_proof, convert_proof_to_compressed};
use light_hasher::{Hasher, Poseidon};
use light_program_test::{
    program_test::LightProgramTest, utils::simulate_cu, AddressWithTree, Indexer, ProgramTestConfig,
    Rpc, RpcError,
};
use light_sdk::{
    address::v2::derive_address,
    instruction::{PackedAccounts, SystemAccountMetaConfig},
};
use num_bigint::BigUint;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::collections::HashMap;
use zk_nullifier::{BATCH_SIZE, NULLIFIER_PREFIX};

#[link(name = "circuit_batch", kind = "static")]
extern "C" {}

rust_witness::witness!(nullifier4);

#[tokio::test]
async fn test_create_batch_nullifier() {
    let config = ProgramTestConfig::new(true, Some(vec![("zk_nullifier", zk_nullifier::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    let secrets: [[u8; 32]; BATCH_SIZE] = [
        generate_random_secret(),
        generate_random_secret(),
        generate_random_secret(),
        generate_random_secret(),
    ];
    let verification_id = Pubkey::new_unique().to_bytes();
    let nullifiers: [[u8; 32]; BATCH_SIZE] = [
        compute_nullifier(&verification_id, &secrets[0]),
        compute_nullifier(&verification_id, &secrets[1]),
        compute_nullifier(&verification_id, &secrets[2]),
        compute_nullifier(&verification_id, &secrets[3]),
    ];

    let mut addresses = Vec::with_capacity(BATCH_SIZE);
    for i in 0..BATCH_SIZE {
        let (addr, _) = derive_address(
            &[
                NULLIFIER_PREFIX,
                nullifiers[i].as_slice(),
                verification_id.as_slice(),
            ],
            &address_tree_info.tree,
            &zk_nullifier::ID,
        );
        addresses.push(addr);
    }

    let instruction = build_create_batch_nullifier_instruction(
        &mut rpc,
        &payer,
        &addresses,
        address_tree_info.clone(),
        &verification_id,
        &nullifiers,
        &secrets,
    )
    .await
    .unwrap();

    let cu = simulate_cu(&mut rpc, &payer, &instruction).await;
    println!("=== Batch (4 nullifiers) CU: {} ===", cu);
    println!("=== CU per nullifier (batch): {} ===", cu / 4);

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    let nullifier_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_nullifier::ID, None, None)
        .await
        .unwrap();
    assert_eq!(nullifier_accounts.value.items.len(), BATCH_SIZE);

    // Duplicate batch should fail
    let dup_instruction = build_create_batch_nullifier_instruction(
        &mut rpc,
        &payer,
        &addresses,
        address_tree_info,
        &verification_id,
        &nullifiers,
        &secrets,
    )
    .await
    .unwrap();

    let result = rpc
        .create_and_send_transaction(&[dup_instruction], &payer.pubkey(), &[&payer])
        .await;
    assert!(result.is_err());
}

fn generate_random_secret() -> [u8; 32] {
    let random_keypair = Keypair::new();
    let mut secret = [0u8; 32];
    secret[1..32].copy_from_slice(&random_keypair.to_bytes()[0..31]);
    secret
}

fn compute_nullifier(verification_id: &[u8; 32], secret: &[u8; 32]) -> [u8; 32] {
    Poseidon::hashv(&[verification_id, secret]).unwrap()
}

async fn build_create_batch_nullifier_instruction<R>(
    rpc: &mut R,
    payer: &Keypair,
    addresses: &[[u8; 32]],
    address_tree_info: light_client::indexer::TreeInfo,
    verification_id: &[u8; 32],
    nullifiers: &[[u8; 32]; BATCH_SIZE],
    secrets: &[[u8; 32]; BATCH_SIZE],
) -> Result<Instruction, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    remaining_accounts.add_pre_accounts_signer(payer.pubkey());
    let config = SystemAccountMetaConfig::new(zk_nullifier::ID);
    remaining_accounts.add_system_accounts_v2(config)?;

    let address_with_trees: Vec<AddressWithTree> = addresses
        .iter()
        .map(|addr| AddressWithTree {
            address: *addr,
            tree: address_tree_info.tree,
        })
        .collect();

    let rpc_result = rpc
        .get_validity_proof(vec![], address_with_trees, None)
        .await?
        .value;

    let packed_address_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees;

    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let zk_proof = generate_batch_zk_proof(verification_id, nullifiers, secrets);

    let address_tree_infos: [_; BATCH_SIZE] = [
        packed_address_tree_accounts[0],
        packed_address_tree_accounts[1],
        packed_address_tree_accounts[2],
        packed_address_tree_accounts[3],
    ];

    let (remaining_accounts_metas, system_accounts_offset, _) = remaining_accounts.to_account_metas();

    let instruction_data = zk_nullifier::instruction::CreateBatchNullifier {
        proof: rpc_result.proof,
        address_tree_infos,
        output_state_tree_index,
        system_accounts_offset: system_accounts_offset as u8,
        zk_proof,
        verification_id: *verification_id,
        nullifiers: *nullifiers,
    };

    let accounts = zk_nullifier::accounts::CreateNullifierAccounts {
        signer: payer.pubkey(),
    };

    Ok(Instruction {
        program_id: zk_nullifier::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    })
}

fn generate_batch_zk_proof(
    verification_id: &[u8; 32],
    nullifiers: &[[u8; 32]; BATCH_SIZE],
    secrets: &[[u8; 32]; BATCH_SIZE],
) -> light_compressed_account::instruction_data::compressed_proof::CompressedProof {
    // CARGO_MANIFEST_DIR = programs/zk-nullifier, build is at workspace root
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let zkey_path = format!("{}/../../build/nullifier_4_final.zkey", manifest_dir);

    let mut proof_inputs = HashMap::new();
    proof_inputs.insert(
        "verification_id".to_string(),
        vec![BigUint::from_bytes_be(verification_id).to_string()],
    );

    let nullifier_strings: Vec<String> = nullifiers
        .iter()
        .map(|n| BigUint::from_bytes_be(n).to_string())
        .collect();
    proof_inputs.insert("nullifier".to_string(), nullifier_strings);

    let secret_strings: Vec<String> = secrets
        .iter()
        .map(|s| BigUint::from_bytes_be(s).to_string())
        .collect();
    proof_inputs.insert("secret".to_string(), secret_strings);

    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(nullifier4_witness),
        circuit_inputs,
        zkey_path.clone(),
    )
    .expect("Proof generation failed");

    let is_valid = CircomProver::verify(ProofLib::Arkworks, proof.clone(), zkey_path)
        .expect("Proof verification failed");
    assert!(is_valid);

    compress_proof(&proof.proof)
}

fn compress_proof(
    proof: &circom_prover::prover::circom::Proof,
) -> light_compressed_account::instruction_data::compressed_proof::CompressedProof {
    let (proof_a_uncompressed, proof_b_uncompressed, proof_c_uncompressed) =
        convert_proof(proof).expect("Failed to convert proof");

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

