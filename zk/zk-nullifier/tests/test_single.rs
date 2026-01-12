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
use zk_nullifier::NULLIFIER_PREFIX;

#[link(name = "circuit_single", kind = "static")]
extern "C" {}

rust_witness::witness!(nullifier);

#[tokio::test]
async fn test_create_nullifier() {
    let config = ProgramTestConfig::new(true, Some(vec![("zk_nullifier", zk_nullifier::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();

    let secret = generate_random_secret();
    let verification_id = Pubkey::new_unique().to_bytes();
    let nullifier = compute_nullifier(&verification_id, &secret);

    let (nullifier_address, _) = derive_address(
        &[
            NULLIFIER_PREFIX,
            nullifier.as_slice(),
            verification_id.as_slice(),
        ],
        &address_tree_info.tree,
        &zk_nullifier::ID,
    );

    let instruction = build_create_nullifier_instruction(
        &mut rpc,
        &payer,
        &nullifier_address,
        address_tree_info.clone(),
        &verification_id,
        &nullifier,
        &secret,
    )
    .await
    .unwrap();

    let cu = simulate_cu(&mut rpc, &payer, &instruction).await;
    println!("=== Single nullifier CU: {} ===", cu);

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();

    let nullifier_accounts = rpc
        .get_compressed_accounts_by_owner(&zk_nullifier::ID, None, None)
        .await
        .unwrap();
    assert_eq!(nullifier_accounts.value.items.len(), 1);

    // Duplicate should fail
    let dup_instruction = build_create_nullifier_instruction(
        &mut rpc,
        &payer,
        &nullifier_address,
        address_tree_info,
        &verification_id,
        &nullifier,
        &secret,
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

async fn build_create_nullifier_instruction<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    verification_id: &[u8; 32],
    nullifier: &[u8; 32],
    secret: &[u8; 32],
) -> Result<Instruction, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(zk_nullifier::ID);
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
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;

    let zk_proof = generate_zk_proof(verification_id, nullifier, secret);

    let instruction_data = zk_nullifier::instruction::CreateNullifier {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        zk_proof,
        verification_id: *verification_id,
        nullifier: *nullifier,
    };

    let accounts = zk_nullifier::accounts::CreateNullifierAccounts {
        signer: payer.pubkey(),
    };

    Ok(Instruction {
        program_id: zk_nullifier::ID,
        accounts: [
            accounts.to_account_metas(None),
            remaining_accounts.to_account_metas().0,
        ]
        .concat(),
        data: instruction_data.data(),
    })
}

fn generate_zk_proof(
    verification_id: &[u8; 32],
    nullifier: &[u8; 32],
    secret: &[u8; 32],
) -> light_compressed_account::instruction_data::compressed_proof::CompressedProof {
    let zkey_path = "./build/nullifier_final.zkey".to_string();

    let mut proof_inputs = HashMap::new();
    proof_inputs.insert(
        "verification_id".to_string(),
        vec![BigUint::from_bytes_be(verification_id).to_string()],
    );
    proof_inputs.insert(
        "nullifier".to_string(),
        vec![BigUint::from_bytes_be(nullifier).to_string()],
    );
    proof_inputs.insert(
        "secret".to_string(),
        vec![BigUint::from_bytes_be(secret).to_string()],
    );

    let circuit_inputs = serde_json::to_string(&proof_inputs).unwrap();
    let proof = CircomProver::prove(
        ProofLib::Arkworks,
        WitnessFn::RustWitness(nullifier_witness),
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

