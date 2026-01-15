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