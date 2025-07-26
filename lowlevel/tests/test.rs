#![cfg(feature = "test-sbf")]

use anchor_lang::{InstructionData, ToAccountMetas};
use lowlevel::FIRST_SEED;
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

#[tokio::test]
async fn test_create_address_and_output_without_address() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![("lowlevel", lowlevel::ID)]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();

    let (address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &lowlevel::ID,
    );

    // Test data for the low-level instruction
    let encrypted_utxo = vec![1, 2, 3, 4, 5]; // Example encrypted UTXO data
    let output_utxo_hash = [42u8; 32]; // Example hash

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

    let instruction_data = lowlevel::instruction::CreateAddressAndOutputWithoutAddress {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        encrypted_utxo,
        output_utxo_hash,
    };

    let accounts = lowlevel::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
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
