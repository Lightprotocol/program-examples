pragma circom 2.0.0;

include "./nullifier.circom";

component main { public [verification_id, nullifier] } = Nullifier();
