pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";

// Based on Tornado Cash Nova's keypair design
// Proves knowledge of a private key that corresponds to a public key commitment
template Keypair() {
    // Private inputs - only the credential holder knows these
    signal input privateKey;

    // Public output - this is what gets stored on-chain
    signal output publicKey;

    component hasher = Poseidon(1);
    hasher.inputs[0] <== privateKey;
    publicKey <== hasher.out;
}

// Proves ownership of a credential by proving knowledge of the private key
template CredentialOwnership() {
    // Private inputs
    signal input credentialPrivateKey;

    // Public inputs
    signal input credentialPublicKey; // This is stored on-chain (the commitment)
    signal input issuer;

    // Verify the private key corresponds to the public key
    component keypair = Keypair();
    keypair.privateKey <== credentialPrivateKey;

    // Ensure the computed public key matches the one stored on-chain
    credentialPublicKey === keypair.publicKey;

    // Output the credential hash for further processing
    signal output credentialHash;
    component credHasher = Poseidon(2);
    credHasher.inputs[0] <== issuer;
    credHasher.inputs[1] <== credentialPublicKey;
    credentialHash <== credHasher.out;
}