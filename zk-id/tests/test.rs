// #![cfg(feature = "test-sbf")]

use anchor_lang::{InstructionData, ToAccountMetas};
use light_compressed_account::{
    compressed_account::hash_with_hashed_values, hash_to_bn254_field_size_be,
};
use light_hasher::{Hasher, Poseidon};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v1::derive_address,
    instruction::{PackedAccounts, SystemAccountMetaConfig},
};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};
use zk_id::FIRST_SEED;

#[tokio::test]
async fn test_create_address_and_output_without_address() {
    let config = ProgramTestConfig::new(true, Some(vec![("lowlevel", lowlevel::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();

    let (address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &lowlevel::ID,
    );

    // Test data for the low-level instruction
    let encrypted_utxo = vec![1; 256]; // Example encrypted UTXO data
    let mut output_utxo_hash = [42u8; 32]; // Example hash
    output_utxo_hash[0] = 0;
    // Create the low-level account
    create_address_and_output_without_address(
        &mut rpc,
        &payer,
        &address,
        address_tree_info,
        encrypted_utxo,
        output_utxo_hash,
    )
    .await
    .unwrap();

    let program_compressed_accounts = rpc
        .get_compressed_accounts_by_owner(&lowlevel::ID, None, None)
        .await
        .unwrap();
    let compressed_account = program_compressed_accounts.value.items[0].clone();
    println!("{:?}", compressed_account);
    let data_hash = compressed_account.data.as_ref().unwrap().data_hash;
    println!("data hash {:?}", data_hash);
    let discriminator = compressed_account.data.as_ref().unwrap().discriminator;
    println!("discriminator {:?}", discriminator);
    let hashed_owner = hash_to_bn254_field_size_be(compressed_account.owner.as_ref());
    println!(
        "hashed owner {:?}",
        hash_to_bn254_field_size_be(compressed_account.owner.as_ref())
    );
    let hashed_merkle_tree =
        hash_to_bn254_field_size_be(compressed_account.tree_info.tree.as_ref());
    println!("hashed merkle_tree {:?}", hashed_merkle_tree);
    let leaf_index = compressed_account.leaf_index;
    println!("leaf index le {}", compressed_account.leaf_index);
    let mut discriminator_bytes = [0u8; 32];
    discriminator_bytes[24..].copy_from_slice(discriminator.as_slice());
    discriminator_bytes[23] = 2; // Domain separator for discriminator.
    let compressed_account_hash = Poseidon::hashv(&[
        hashed_owner.as_slice(),
        leaf_index.to_le_bytes().as_slice(), // this is a footgun, we serialize leaf_index le but Poseidon::hashv expects be bytes.
        // Ts poseidon hashers expect le input, to be consistent you need to provide the be number to the ts hasher.
        // It is a mistake in our hashing, not dangerous because we just use it to make the hash unique which holds true since it is consistent, but difficult to change, we fixed it in V2 trees.
        // For the circuit this means that you need to pass leaf index twice once in be and use it to compute the light leaf hash, once le to verify the merkle proof.
        hashed_merkle_tree.as_slice(),
        discriminator_bytes.as_slice(),
        data_hash.as_slice(),
    ])
    .unwrap();
    assert_eq!(compressed_account_hash, compressed_account.hash);
    let compressed_account_hash = hash_with_hashed_values(
        &0,
        None,
        Some((discriminator.as_slice(), data_hash.as_slice())),
        &hashed_owner,
        &hashed_merkle_tree,
        &leaf_index,
        false,
    )
    .unwrap();
    assert_eq!(compressed_account_hash, compressed_account.hash);
}

async fn create_address_and_output_without_address<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: light_client::indexer::TreeInfo,
    encrypted_utxo: Vec<u8>,
    output_utxo_hash: [u8; 32],
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(lowlevel::ID);
    remaining_accounts.add_system_accounts(config);

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

    let input_state_tree_info = rpc.get_random_state_tree_info()?;
    let input_root_index = 0u16;

    let instruction_data = lowlevel::instruction::CreateAddressAndOutputWithoutAddress {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        input_root_index,
        encrypted_utxo,
        output_utxo_hash,
    };

    let accounts = lowlevel::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
        input_merkle_tree: input_state_tree_info.tree,
    };

    let instruction = Instruction {
        program_id: lowlevel::ID,
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
