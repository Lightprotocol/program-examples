fn main() {
    // Transpile the WebAssembly witness generators for Circom circuits
    // into a native library for use with circom-prover
    rust_witness::transpile::transpile_wasm("./build/compressed_account_merkle_proof_js".to_string());
}
