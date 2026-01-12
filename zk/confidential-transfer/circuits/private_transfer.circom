pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";
include "../node_modules/circomlib/circuits/bitify.circom";
include "./compressed_account.circom";
include "./merkle_proof.circom";

// Commitment = Poseidon(amount, blinding)
template BalanceCommitment() {
    signal input amount;
    signal input blinding;
    signal output commitment;

    component hasher = Poseidon(2);
    hasher.inputs[0] <== amount;
    hasher.inputs[1] <== blinding;
    commitment <== hasher.out;
}

// Main Circuit: Private Transfer
// Proves a valid balance transfer without revealing amounts
template PrivateTransfer(levels) {
    // ============ PUBLIC INPUTS ============
    // Account identifiers (for merkle proof verification)
    signal input owner_hashed;
    signal input merkle_tree_hashed;
    signal input discriminator;
    signal input mint_hashed;

    // Merkle tree root
    signal input expectedRoot;

    // Nullifier (prevents double-spend of sender's commitment)
    signal input nullifier;

    // New commitments
    signal input new_sender_commitment;
    signal input receiver_commitment;

    // ============ PRIVATE INPUTS ============
    // Sender's current balance
    signal input sender_amount;
    signal input sender_blinding;

    // Transfer details
    signal input transfer_amount;

    // New blinding factors
    signal input new_sender_blinding;
    signal input receiver_blinding;

    // Account position in merkle tree
    signal input leaf_index;
    signal input account_leaf_index;
    signal input address;

    // Merkle proof
    signal input pathElements[levels];

    // Step 1: Verify sender's current commitment
    component senderCommitment = BalanceCommitment();
    senderCommitment.amount <== sender_amount;
    senderCommitment.blinding <== sender_blinding;

    // Step 2: Compute and verify nullifier
    // Nullifier = Poseidon(commitment, blinding) - unique per commitment
    component nullifierHasher = Poseidon(2);
    nullifierHasher.inputs[0] <== senderCommitment.commitment;
    nullifierHasher.inputs[1] <== sender_blinding;
    nullifier === nullifierHasher.out;

    // Step 3: Range checks - verify sender has enough funds
    // Use bit decomposition to prove sender_amount >= transfer_amount
    // We prove this by showing (sender_amount - transfer_amount) fits in 64 bits (no underflow)
    signal balance_after_transfer;
    balance_after_transfer <== sender_amount - transfer_amount;

    // Decompose to 64 bits - this implicitly proves balance_after_transfer >= 0
    component balanceCheck = Num2Bits(64);
    balanceCheck.in <== balance_after_transfer;

    // Also verify transfer_amount > 0
    component transferCheck = Num2Bits(64);
    transferCheck.in <== transfer_amount;

    // Verify transfer_amount is non-zero by checking at least one bit is set
    signal transfer_is_nonzero;
    component isZero = IsZero();
    isZero.in <== transfer_amount;
    transfer_is_nonzero <== 1 - isZero.out;
    transfer_is_nonzero === 1;

    // Step 4: Verify new sender commitment is correct
    component newSenderCommitment = BalanceCommitment();
    newSenderCommitment.amount <== balance_after_transfer;
    newSenderCommitment.blinding <== new_sender_blinding;
    new_sender_commitment === newSenderCommitment.commitment;

    // Step 5: Verify receiver commitment is correct
    component receiverCommitment = BalanceCommitment();
    receiverCommitment.amount <== transfer_amount;
    receiverCommitment.blinding <== receiver_blinding;
    receiver_commitment === receiverCommitment.commitment;

    // Step 6: Compute data hash for the sender's account
    // data_hash = Poseidon(mint_hashed, sender_commitment)
    component dataHasher = Poseidon(2);
    dataHasher.inputs[0] <== mint_hashed;
    dataHasher.inputs[1] <== senderCommitment.commitment;
    signal data_hash <== dataHasher.out;

    // Step 7: Compute compressed account hash
    component accountHasher = CompressedAccountHash();
    accountHasher.owner_hashed <== owner_hashed;
    accountHasher.leaf_index <== account_leaf_index;
    accountHasher.address <== address;
    accountHasher.merkle_tree_hashed <== merkle_tree_hashed;
    accountHasher.discriminator <== discriminator;
    accountHasher.data_hash <== data_hash;

    // Step 8: Verify Merkle proof
    component merkleProof = MerkleProof(levels);
    merkleProof.leaf <== accountHasher.hash;
    merkleProof.pathElements <== pathElements;
    merkleProof.leafIndex <== leaf_index;
    merkleProof.root === expectedRoot;
}

// Main component with 26 levels (typical for Solana state trees)
// Note: new_sender_commitment is constrained but not public (for stack efficiency)
// The sender tracks their change commitment off-chain
component main {
    public [
        owner_hashed,
        merkle_tree_hashed,
        discriminator,
        mint_hashed,
        expectedRoot,
        nullifier,
        receiver_commitment
    ]
} = PrivateTransfer(26);
