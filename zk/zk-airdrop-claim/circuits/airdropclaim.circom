pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/poseidon.circom";
include "merkle_proof.circom";

// Anonymous Airdrop Claim Circuit
// Proves:
// 1. Claimant knows private key for an address in the eligibility Merkle tree
// 2. The address is entitled to a specific amount
// 3. Nullifier is correctly derived (prevents double-claims)
// 4. Recipient is bound to the proof (prevents front-running)
//
// Privacy guarantee: Observer sees "Someone claimed X tokens to address Y"
// but cannot tell which eligible address from the snapshot is claiming.
template AirdropClaim(levels) {
    // ============ PUBLIC INPUTS ============
    // Merkle root of the eligibility tree (each leaf = Poseidon(address, amount))
    signal input eligibilityRoot;
    // Nullifier = Poseidon(airdropId, privateKey) - prevents double-claim
    signal input nullifier;
    // Recipient address (can be different from eligible address)
    signal input recipient;
    // Unique identifier for this airdrop
    signal input airdropId;
    // Amount the claimant is entitled to (part of Merkle leaf)
    signal input amount;

    // ============ PRIVATE INPUTS ============
    // Claimant's secret key (derives their eligible address)
    signal input privateKey;
    // Merkle proof siblings
    signal input pathElements[levels];
    // Leaf position in the tree
    signal input leafIndex;

    // Step 1: Derive eligible address from private key
    // eligibleAddress = Poseidon(privateKey)
    component addressHasher = Poseidon(1);
    addressHasher.inputs[0] <== privateKey;
    signal eligibleAddress <== addressHasher.out;

    // Step 2: Compute the leaf = Poseidon(eligibleAddress, amount)
    // This is what's stored in the eligibility Merkle tree
    component leafHasher = Poseidon(2);
    leafHasher.inputs[0] <== eligibleAddress;
    leafHasher.inputs[1] <== amount;
    signal leaf <== leafHasher.out;

    // Step 3: Verify nullifier is correctly derived
    // nullifier = Poseidon(airdropId, privateKey)
    component nullifierHasher = Poseidon(2);
    nullifierHasher.inputs[0] <== airdropId;
    nullifierHasher.inputs[1] <== privateKey;
    nullifier === nullifierHasher.out;

    // Step 4: Verify Merkle inclusion proof
    // Proves the (address, amount) pair is in the eligibility tree
    component merkleProof = MerkleProof(levels);
    merkleProof.leaf <== leaf;
    merkleProof.pathElements <== pathElements;
    merkleProof.leafIndex <== leafIndex;
    merkleProof.root === eligibilityRoot;

    // Step 5: Bind recipient to proof (prevents front-running)
    // The recipient is a public input, so changing it invalidates the proof
    signal recipientSquare;
    recipientSquare <== recipient * recipient;
}

// 20 levels supports up to 2^20 = ~1 million eligible addresses
component main {
    public [
        eligibilityRoot,
        nullifier,
        recipient,
        airdropId,
        amount
    ]
} = AirdropClaim(20);

