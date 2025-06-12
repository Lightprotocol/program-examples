use account_comparison::AccountData;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;

#[test]
fn test_solana_account() {
    let user = Keypair::new();

    let mut svm = LiteSVM::new();
    svm.add_program_from_file(
        account_comparison::ID,
        "../../target/deploy/account_comparison.so",
    )
    .unwrap();
    svm.airdrop(&user.pubkey(), 1_000_000_000_000).unwrap();

    let account_pda = Pubkey::find_program_address(
        &[b"account", user.pubkey().as_ref()],
        &account_comparison::id(),
    )
    .0;

    create_solana_account(&mut svm, &user, &account_pda);

    let account = svm.get_account(&account_pda).unwrap();
    let data_account = AccountData::deserialize(&mut &account.data[8..]).unwrap();
    println!("data_account {:?}", data_account);
    assert_eq!(data_account.name, "Heinrich".to_string());
    assert_eq!(data_account.data, [1u8; 128]);

    update_solana_account(&mut svm, &user, &account_pda, [2u8; 128]);

    let account = svm.get_account(&account_pda).unwrap();
    let data_account = AccountData::deserialize(&mut &account.data[8..]).unwrap();
    println!("data_account {:?}", data_account);
    assert_eq!(data_account.name, "Heinrich".to_string());
    assert_eq!(data_account.data, [2u8; 128]);
}

fn create_solana_account(svm: &mut LiteSVM, user: &Keypair, account_pda: &Pubkey) {
    let instruction = account_comparison::instruction::CreateAccount {
        name: "Heinrich".to_string(),
    };
    let accounts = account_comparison::accounts::CreateAccount {
        user: user.pubkey(),
        account: *account_pda,
        system_program: Pubkey::default(),
    };

    let instruction = Instruction {
        program_id: account_comparison::id(),
        accounts: accounts.to_account_metas(Some(true)),
        data: instruction.data(),
    };

    let tx = Transaction::new(
        &[&user],
        Message::new(&[instruction], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );
    let tx_res = svm.send_transaction(tx).unwrap();
    println!("{}", tx_res.pretty_logs());
}

fn update_solana_account(svm: &mut LiteSVM, user: &Keypair, account_pda: &Pubkey, data: [u8; 128]) {
    let instruction_data = account_comparison::instruction::UpdateData { data };
    let accounts = account_comparison::accounts::UpdateData {
        user: user.pubkey(),
        account: *account_pda,
    };

    let instruction = Instruction {
        program_id: account_comparison::id(),
        accounts: accounts.to_account_metas(Some(true)),
        data: instruction_data.data(),
    };

    let tx = Transaction::new(
        &[&user],
        Message::new(&[instruction], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );
    let tx_res = svm.send_transaction(tx).unwrap();
    println!("{}", tx_res.pretty_logs());
}
