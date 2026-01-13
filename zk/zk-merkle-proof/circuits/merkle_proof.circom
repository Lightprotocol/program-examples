pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/bitify.circom";
include "../node_modules/circomlib/circuits/switcher.circom";

// Merkle Proof Verification Template
// Verifies that a leaf is in a Merkle tree with a given root
template MerkleProof(levels) {
    signal input leaf;
    signal input pathElements[levels];
    signal input leafIndex;
    signal output root;

    component switcher[levels];
    component hasher[levels];
    component indexBits = Num2Bits(levels);
    indexBits.in <== leafIndex;

    for (var i = 0; i < levels; i++) {
        switcher[i] = Switcher();
        switcher[i].L <== i == 0 ? leaf : hasher[i - 1].out;
        switcher[i].R <== pathElements[i];
        switcher[i].sel <== indexBits.out[i];

        hasher[i] = Poseidon(2);
        hasher[i].inputs[0] <== switcher[i].outL;
        hasher[i].inputs[1] <== switcher[i].outR;
    }

    root <== hasher[levels - 1].out;
}

template CompressedAccountHash() {
    signal input owner_hashed;
    signal input leaf_index;
    signal input merkle_tree_hashed;
    signal input address;
    signal input discriminator;
    signal input data_hash;
    signal output hash;

    component poseidon = Poseidon(6);
    poseidon.inputs[0] <== owner_hashed;
    poseidon.inputs[1] <== leaf_index;
    poseidon.inputs[2] <== merkle_tree_hashed;
    poseidon.inputs[3] <== address;
    poseidon.inputs[4] <== discriminator + 36893488147419103232;
    poseidon.inputs[5] <== data_hash;
    hash <== poseidon.out;
}

template CompressedCompressedAccountMerkleProof(levels) {
    signal input owner_hashed;
    signal input merkle_tree_hashed;
    signal input discriminator;
    signal input data_hash;
    signal input expectedRoot;

    signal input leaf_index;
    signal input account_leaf_index;
    signal input address;
    signal input pathElements[levels];

    component accountHasher = CompressedAccountHash();
    accountHasher.owner_hashed <== owner_hashed;
    accountHasher.leaf_index <== account_leaf_index;
    accountHasher.address <== address;
    accountHasher.merkle_tree_hashed <== merkle_tree_hashed;
    accountHasher.discriminator <== discriminator;
    accountHasher.data_hash <== data_hash;

    component merkleProof = MerkleProof(levels);
    merkleProof.leaf <== accountHasher.hash;
    merkleProof.pathElements <== pathElements;
    merkleProof.leafIndex <== leaf_index;
    merkleProof.root === expectedRoot;
}

component main {
    public [owner_hashed, merkle_tree_hashed, discriminator, data_hash, expectedRoot]
} = CompressedAccountMerkleProof(26);
