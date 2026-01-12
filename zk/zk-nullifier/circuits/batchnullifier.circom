pragma circom 2.0.0;

include "./nullifier.circom";

// Batch nullifier: proves n nullifiers with single proof
// More efficient than n separate proofs
template BatchNullifier(n) {
    signal input verification_id;  // public: shared context
    signal input nullifier[n];     // public: n nullifier hashes
    signal input secret[n];        // private: n secrets

    component nullifiers[n];
    for (var i = 0; i < n; i++) {
        nullifiers[i] = Nullifier();
        nullifiers[i].verification_id <== verification_id;
        nullifiers[i].nullifier <== nullifier[i];
        nullifiers[i].secret <== secret[i];
    }
}

component main { public [verification_id, nullifier] } = BatchNullifier(4);
