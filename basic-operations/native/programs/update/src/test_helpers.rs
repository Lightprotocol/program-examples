use borsh::BorshSerialize;
use light_program_test::{AddressWithTree, Indexer, LightProgramTest, Rpc, RpcError};
use light_sdk::instruction::{PackedAccounts, SystemAccountMetaConfig};
use crate::{CreateInstructionData, InstructionType, ID};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

pub async fn create_compressed_account(
    payer: &Keypair,
    rpc: &mut LightProgramTest,
    merkle_tree_pubkey: &Pubkey,
    address_tree_pubkey: Pubkey,
    address: [u8; 32],
    message: String,
) -> Result<(), RpcError> {
    let system_account_meta_config = SystemAccountMetaConfig::new(ID);
    let mut accounts = PackedAccounts::default();
    accounts.add_pre_accounts_signer(payer.pubkey());
    accounts.add_system_accounts_v2(system_account_meta_config)?;

    let rpc_result = rpc
        .get_validity_proof(
            vec![],
            vec![AddressWithTree {
                address,
                tree: address_tree_pubkey,
            }],
            None,
        )
        .await?
        .value;

    let output_state_tree_index = accounts.insert_or_get(*merkle_tree_pubkey);
    let packed_address_tree_info = rpc_result.pack_tree_infos(&mut accounts).address_trees[0];
    let (account_metas, _, _) = accounts.to_account_metas();

    let instruction_data = CreateInstructionData {
        proof: rpc_result.proof,
        address_tree_info: packed_address_tree_info,
        output_state_tree_index: output_state_tree_index,
        message,
    };
    let inputs = instruction_data.try_to_vec().unwrap();

    let instruction = Instruction {
        program_id: ID,
        accounts: account_metas,
        data: [
            &[InstructionType::Create as u8][..],
            &inputs[..],
        ]
        .concat(),
    };

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[payer])
        .await?;
    Ok(())
}
