pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";

// Single nullifier: proves nullifier = Poseidon(verification_id, secret)
template Nullifier() {
    signal input verification_id;
    signal input nullifier;
    signal input secret;

    component hasher = Poseidon(2);
    hasher.inputs[0] <== verification_id;
    hasher.inputs[1] <== secret;
    nullifier === hasher.out;
}

// Batch nullifier: proves n nullifiers with single proof
template BatchNullifier(n) {
    signal input verification_id;
    signal input nullifier[n];
    signal input secret[n];

    component nullifiers[n];
    for (var i = 0; i < n; i++) {
        nullifiers[i] = Nullifier();
        nullifiers[i].verification_id <== verification_id;
        nullifiers[i].nullifier <== nullifier[i];
        nullifiers[i].secret <== secret[i];
    }
}
