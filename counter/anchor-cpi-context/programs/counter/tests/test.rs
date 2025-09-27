// #![cfg(feature = "test-sbf")]

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use counter::CounterAccount;
use light_client::indexer::{CompressedAccount, TreeInfo};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, Rpc, RpcError,
};
use light_sdk::address::v2::derive_address;
use light_sdk::instruction::{
    account_meta::CompressedAccountMeta, PackedAccounts, SystemAccountMetaConfig,
};
use solana_sdk::{
    instruction::Instruction,
    signature::{Keypair, Signature, Signer},
};

#[tokio::test]
async fn test_counter() {
    let config = ProgramTestConfig::new_v2(true, Some(vec![("counter", counter::ID)]));
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    let address_tree_info = rpc.get_address_tree_v2();
    let (address, _seed) = derive_address(
        &[b"counter".as_slice(), payer.pubkey().as_ref()],
        &address_tree_info.tree,
        &counter::ID,
    );
    println!(
        "address_tree_info.tree.to_bytes() {:?}",
        address_tree_info.tree.to_bytes()
    );

    // Create the counter.
    create_counter(&mut rpc, &payer, &address, address_tree_info)
        .await
        .unwrap();

    // Check that it was created correctly.
    let compressed_account = rpc
        .get_compressed_account(address, None)
        .await
        .unwrap()
        .value;
    assert_eq!(compressed_account.leaf_index, 0);
    let counter = &compressed_account.data.as_ref().unwrap().data;
    let counter = CounterAccount::deserialize(&mut &counter[..]).unwrap();
    assert_eq!(counter.value, 0);

    // Increment the counter.
    increment_counter(&mut rpc, &payer, &compressed_account)
        .await
        .unwrap();
}

async fn create_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    address: &[u8; 32],
    address_tree_info: TreeInfo,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let mut remaining_accounts = PackedAccounts::default();
    let config = SystemAccountMetaConfig::new(counter::ID);
    remaining_accounts.add_system_accounts_v2(config).unwrap();

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                tree: address_tree_info.tree,
                address: *address,
            }],
            None,
        )
        .await?
        .value;
    let output_state_tree_index = rpc
        .get_random_state_tree_info()?
        .pack_output_tree_index(&mut remaining_accounts)?;
    let packed_address_tree_info = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .address_trees[0];

    let instruction_data = counter::instruction::CreateCounter {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index,
    };

    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}

#[allow(clippy::too_many_arguments)]
async fn increment_counter<R>(
    rpc: &mut R,
    payer: &Keypair,
    compressed_account: &CompressedAccount,
) -> Result<Signature, RpcError>
where
    R: Rpc + Indexer,
{
    let hash = compressed_account.hash;

    let rpc_result = rpc
        .get_validity_proof(vec![hash], vec![], None)
        .await?
        .value;

    let mut remaining_accounts = PackedAccounts::default();
    let packed_tree_accounts = rpc_result
        .pack_tree_infos(&mut remaining_accounts)
        .state_trees
        .unwrap();
    let mut config = SystemAccountMetaConfig::new(counter::ID);
    config.cpi_context = rpc_result.accounts[0].tree_info.cpi_context;
    remaining_accounts.add_system_accounts_v2(config).unwrap();

    let counter_account =
        CounterAccount::deserialize(&mut compressed_account.data.as_ref().unwrap().data.as_slice())
            .unwrap();

    let account_meta = CompressedAccountMeta {
        tree_info: packed_tree_accounts.packed_tree_infos[0],
        address: compressed_account.address.unwrap(),
        output_state_tree_index: packed_tree_accounts.output_tree_index,
    };

    let instruction_data = counter::instruction::ChangeOwnerWithCpiContext {
        proof: rpc_result.proof,
        counter_value: counter_account.value,
        account_meta,
    };

    let accounts = counter::accounts::GenericAnchorAccounts {
        signer: payer.pubkey(),
    };

    let (remaining_accounts_metas, _, _) = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: counter::ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            remaining_accounts_metas,
        ]
        .concat(),
        data: instruction_data.data(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await
}
