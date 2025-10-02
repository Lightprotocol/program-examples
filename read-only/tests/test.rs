#![cfg(feature = "test-sbf")]

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use light_client::indexer::{CompressedAccount, TreeInfo};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::{
    address::v1::derive_address,
    instruction::{account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig},
};
use read_only::{DataAccount, ExistingCompressedAccountIxData, FIRST_SEED};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};

#[tokio::test]
async fn test_read_compressed_account() {
    // Read only is only supported for v2 state trees.
    let config = ProgramTestConfig::new_v2(true, Some(vec![("read_only", read_only::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v1();

    let (address, _) = derive_address(
        &[FIRST_SEED, payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &read_only::ID,
    );

    // Create the compressed account
    create_compressed_account(
        &mut rpc,
        &payer,
        &address,
        address_tree_info,
        "Hello, World!".to_string(),
    )
    .await
    .unwrap();

    // Check that it was created correctly
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;

    assert_eq!(compressed_account.leaf_index, 0);
    let data = &compressed_account.data.as_ref().unwrap().data;
    let account_data = DataAccount::deserialize(&mut &data[..]).unwrap();
    assert_eq!(account_data.owner, payer.pubkey());
    assert_eq!(account_data.message, "Hello, World!");

    // Test reading the compressed account
    read_compressed_account(
        &mut rpc,
        &payer,
        &compressed_account,
        "Hello, World!".to_string(),
    )
    .await
    .unwrap();
}

async fn create_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(read_only::ID);
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

    let instruction_data = read_only::instruction::CreateCompressedAccount {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_accounts[0],
        output_state_tree_index,
        message,
    };
    let accounts = read_only::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: read_only::ID,
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

async fn read_compressed_account<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
    message: String,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(read_only::ID);
    remaining_accounts.add_system_accounts(config);

    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let packed_tree_accounts = rpc_result.pack_tree_infos(&mut remaining_accounts);
    let packed_state_tree_accounts = packed_tree_accounts.state_trees.unwrap();

    let account_meta = CompressedAccountMeta {
        tree_info: packed_state_tree_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: 0, // not used
    };

    let instruction_data = read_only::instruction::Read {
        proof: rpc_result.proof,
        existing_account: ExistingCompressedAccountIxData {
            account_meta,
            message,
        },
    };

    let accounts = read_only::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let instruction = Instruction {
        program_id: read_only::ID,
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
