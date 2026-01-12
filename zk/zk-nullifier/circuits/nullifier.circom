pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";

// Proves: nullifier === Poseidon(verification_id, secret)
template Nullifier() {
    signal input verification_id;  // public: context (vote ID, airdrop ID, etc.)
    signal input nullifier;        // public: prevents double-spend
    signal input secret;           // private: only owner knows

    component hasher = Poseidon(2);
    hasher.inputs[0] <== verification_id;
    hasher.inputs[1] <== secret;
    nullifier === hasher.out;
}

component main { public [verification_id, nullifier] } = Nullifier();
