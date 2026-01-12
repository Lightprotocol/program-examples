pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "compressed_account.circom";
include "merkletree.circom";

// Mixer Withdraw Circuit
// Proves knowledge of nullifier and secret for a commitment stored in Light Protocol state tree
template MixerWithdraw(levels) {
    // ============ PUBLIC INPUTS ============
    signal input owner_hashed;
    signal input merkle_tree_hashed;
    signal input discriminator;
    signal input expectedRoot;
    signal input nullifierHash;
    signal input recipient_hashed;  // Hashed recipient to fit in field

    // ============ PRIVATE INPUTS ============
    signal input nullifier;
    signal input secret;
    signal input leaf_index;           // Raw leaf index for Merkle proof (0, 1, 2, ...)
    signal input account_leaf_index;   // SDK-formatted leaf index for account hash (32-byte encoded)
    signal input address;
    signal input pathElements[levels];

    // Step 1: Compute commitment = Poseidon(nullifier, secret)
    component commitmentHasher = Poseidon(2);
    commitmentHasher.inputs[0] <== nullifier;
    commitmentHasher.inputs[1] <== secret;
    signal commitment <== commitmentHasher.out;

    // Step 2: Verify nullifierHash = Poseidon(nullifier)
    component nullifierHasher = Poseidon(1);
    nullifierHasher.inputs[0] <== nullifier;
    nullifierHash === nullifierHasher.out;

    // Step 3: Compute data_hash = Poseidon(commitment)
    // LightHasher derive adds a Poseidon layer for the account's data_hash
    component dataHasher = Poseidon(1);
    dataHasher.inputs[0] <== commitment;
    signal data_hash <== dataHasher.out;

    // Step 4: Compute Light Protocol compressed account hash
    // The data_hash is what Light Protocol stores in the Merkle tree
    component accountHasher = CompressedAccountHash();
    accountHasher.owner_hashed <== owner_hashed;
    accountHasher.leaf_index <== account_leaf_index;  // SDK-formatted 32-byte encoding
    accountHasher.merkle_tree_hashed <== merkle_tree_hashed;
    accountHasher.address <== address;
    accountHasher.discriminator <== discriminator;
    accountHasher.data_hash <== data_hash;

    // Step 5: Verify Merkle proof
    component merkleProof = MerkleProof(levels);
    merkleProof.leaf <== accountHasher.hash;
    merkleProof.pathElements <== pathElements;
    merkleProof.leafIndex <== leaf_index;  // Raw index for path bits
    merkleProof.root === expectedRoot;

    // Step 6: Bind recipient to proof (prevents front-running)
    signal recipientSquare;
    recipientSquare <== recipient_hashed * recipient_hashed;
}

component main {
    public [
        owner_hashed,
        merkle_tree_hashed,
        discriminator,
        expectedRoot,
        nullifierHash,
        recipient_hashed
    ]
} = MixerWithdraw(26);
