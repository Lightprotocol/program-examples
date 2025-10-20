pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";

// Compressed Account Hash Template
// Computes the hash of a compressed account
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
    poseidon.inputs[4] <== discriminator + 36893488147419103232; // + discriminator domain
    poseidon.inputs[5] <== data_hash;

    hash <== poseidon.out;
}