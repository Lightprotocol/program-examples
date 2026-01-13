pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/bitify.circom";
include "../node_modules/circomlib/circuits/switcher.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

// Merkle Proof Verification Template
// Verifies that a leaf is in a Merkle tree with a given root
template MerkleProof(levels) {
    signal input leaf;
    signal input pathElements[levels];
    signal input leafIndex;
    signal output root;

    // Range check: leafIndex < 2^levels
    component indexRange = LessThan(levels + 1);
    indexRange.in[0] <== leafIndex;
    indexRange.in[1] <== 1 << levels;
    indexRange.out === 1;

    component switcher[levels];
    component hasher[levels];

    component indexBits = Num2Bits_strict(levels);
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