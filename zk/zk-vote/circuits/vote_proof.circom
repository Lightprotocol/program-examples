pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";
include "./credential.circom";
include "./compressed_account.circom";
include "./merkle_proof.circom";

// Vote Proof Circuit: Anonymous Voting with ZK Credentials
// Proves: "I own a valid voter credential for this poll" without revealing identity
// Reveals: vote_choice (public), nullifier (prevents double-voting)
template VoteProof(levels) {
    // ============ PUBLIC INPUTS ============
    // Account identifiers (for credential verification)
    signal input owner_hashed;           // Program ID hashed
    signal input merkle_tree_hashed;     // State tree pubkey hashed
    signal input discriminator;          // VoterCredential discriminator

    // Poll binding
    signal input poll_id_hashed;         // Poll PDA hashed (binds credential to poll)

    // Merkle tree state
    signal input expectedRoot;           // Current Merkle tree root

    // Nullifier (prevents double-voting)
    signal input nullifier;              // Poseidon(poll_id_hashed, credentialPrivateKey)

    // Vote choice (PUBLIC - this is revealed)
    signal input vote_choice;            // 0, 1, or 2

    // ============ PRIVATE INPUTS ============
    // Credential secret
    signal input credentialPrivateKey;   // Voter's secret key

    // Account position in tree
    signal input leaf_index;             // Position for Merkle proof ordering
    signal input account_leaf_index;     // Compressed account format
    signal input address;                // Credential address

    // Merkle proof
    signal input pathElements[levels];   // Sibling hashes

    // ============ CONSTRAINTS ============

    // Step 1: Derive credential public key from private key
    // Proves knowledge of private key without revealing it
    component keypair = Keypair();
    keypair.privateKey <== credentialPrivateKey;
    signal credential_pubkey <== keypair.publicKey;

    // Step 2: Compute and verify nullifier
    // Nullifier = Poseidon(poll_id_hashed, credentialPrivateKey)
    // This ensures each credential can only vote once per poll
    // Different polls have different poll_id_hashed, so same credential can vote in multiple polls
    component nullifierHasher = Poseidon(2);
    nullifierHasher.inputs[0] <== poll_id_hashed;
    nullifierHasher.inputs[1] <== credentialPrivateKey;
    nullifier === nullifierHasher.out;

    // Step 3: Compute credential data hash
    // VoterCredential stores: poll_id (hashed) + credential_pubkey
    component data_hasher = Poseidon(2);
    data_hasher.inputs[0] <== poll_id_hashed;
    data_hasher.inputs[1] <== credential_pubkey;
    signal data_hash <== data_hasher.out;

    // Step 4: Compute compressed account hash
    // This reconstructs the account hash stored in the Merkle tree
    component accountHasher = CompressedAccountHash();
    accountHasher.owner_hashed <== owner_hashed;
    accountHasher.leaf_index <== account_leaf_index;
    accountHasher.address <== address;
    accountHasher.merkle_tree_hashed <== merkle_tree_hashed;
    accountHasher.discriminator <== discriminator;
    accountHasher.data_hash <== data_hash;

    // Step 5: Verify Merkle proof
    // Proves the credential account exists in the state tree
    component merkleProof = MerkleProof(levels);
    merkleProof.leaf <== accountHasher.hash;
    merkleProof.pathElements <== pathElements;
    merkleProof.leafIndex <== leaf_index;
    merkleProof.root === expectedRoot;

    // Step 6: Validate vote choice (must be 0, 1, or 2)
    component lessThan = LessThan(8);
    lessThan.in[0] <== vote_choice;
    lessThan.in[1] <== 3;
    lessThan.out === 1;
}

// Main component with 26 levels (v1 state tree height)
component main {
    public [
        owner_hashed,
        merkle_tree_hashed,
        discriminator,
        poll_id_hashed,
        expectedRoot,
        nullifier,
        vote_choice
    ]
} = VoteProof(26);
