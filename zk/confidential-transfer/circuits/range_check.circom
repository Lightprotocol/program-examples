pragma circom 2.0.0;

include "../node_modules/circomlib/circuits/comparators.circom";
include "../node_modules/circomlib/circuits/bitify.circom";

// Proves that a value fits within n bits (0 <= value < 2^n)
template RangeCheck(n) {
    signal input value;

    component bits = Num2Bits(n);
    bits.in <== value;
}

// Proves that a >= b for n-bit values
template GreaterEqThan(n) {
    signal input a;
    signal input b;
    signal output out;

    component gte = GreaterEqThan(n);
    gte.in[0] <== a;
    gte.in[1] <== b;
    out <== gte.out;
}

// Proves that a > 0 (non-zero check)
template NonZero() {
    signal input value;
    signal output out;

    component isz = IsZero();
    isz.in <== value;
    out <== 1 - isz.out;
}

// Proves a >= b and constrains the result
// Fails if a < b
template AssertGreaterEqThan(n) {
    signal input a;
    signal input b;

    component gte = GreaterEqThan(n);
    gte.in[0] <== a;
    gte.in[1] <== b;
    gte.out === 1;
}
