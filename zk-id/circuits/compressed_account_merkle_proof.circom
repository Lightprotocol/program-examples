pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "./credential.circom";
include "./compressed_account.circom";
include "./merkle_proof.circom";



// Main Circuit: Compressed Account Merkle Proof Verification
// Computes compressed account hash and verifies it exists in a Merkle tree
template CompressedAccountMerkleProof(levels) {
    // ============ PUBLIC INPUTS ============
    // Account identifiers
    signal input owner_hashed;
    signal input merkle_tree_hashed;
    signal input discriminator;
    signal input issuer_hashed;

    // Merkle tree root
    signal input expectedRoot;

    // Verification context (external nullifier)
    signal input verification_id;

    // Data commitments
    signal input public_encrypted_data_hash;

    // Nullifier (prevents double-spending)
    signal input nullifier;

    // ============ PRIVATE INPUTS ============
    // Credential secret
    signal input credentialPrivateKey;

    // Account position
    signal input leaf_index;
    signal input account_leaf_index;
    signal input address;

    // Merkle proof
    signal input pathElements[levels];

    // Private data
    signal input encrypted_data_hash;

    // Step 1: Verify credential ownership using private key
    component keypair = Keypair();
    keypair.privateKey <== credentialPrivateKey;
    signal credential_pubkey_commitment <== keypair.publicKey;

    // Step 2: Compute and verify nullifier
    // Nullifier = Poseidon(verification_id, credentialPrivateKey)
    // This ensures each credential can only be used once per verification_id
    // without leaking information about the credential itself.
    component nullifierHasher = Poseidon(2);
    nullifierHasher.inputs[0] <== verification_id;
    nullifierHasher.inputs[1] <== credentialPrivateKey;
    nullifier === nullifierHasher.out;

    // Step 3: Compute the credential data hash (used internally for account hash)
    component data_hasher = Poseidon(2);
    data_hasher.inputs[0] <== issuer_hashed;
    data_hasher.inputs[1] <== credential_pubkey_commitment;
    signal data_hash <== data_hasher.out;

    // Step 4: Compute compressed account hash
    component accountHasher = CompressedAccountHash();
    accountHasher.owner_hashed <== owner_hashed;
    accountHasher.leaf_index <== account_leaf_index;
    accountHasher.address <== address;
    accountHasher.merkle_tree_hashed <== merkle_tree_hashed;
    accountHasher.discriminator <== discriminator;
    accountHasher.data_hash <== data_hash;

    // Step 5: Verify Merkle proof
    component merkleProof = MerkleProof(levels);
    merkleProof.leaf <== accountHasher.hash;
    merkleProof.pathElements <== pathElements;
    merkleProof.leafIndex <== leaf_index;
    merkleProof.root === expectedRoot;

    // Step 7: Verify encrypted data hash matches
    public_encrypted_data_hash === encrypted_data_hash;
}

// Main component with 26 levels (typical for Solana state trees)
component main {
    public [
        owner_hashed,
        merkle_tree_hashed,
        discriminator,
        issuer_hashed,
        expectedRoot,
        verification_id,
        public_encrypted_data_hash,
        nullifier
    ]
} = CompressedAccountMerkleProof(26);
