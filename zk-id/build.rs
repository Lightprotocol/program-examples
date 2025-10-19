use groth16_solana::vk_parser::generate_vk_file;

fn main() {
    println!("cargo:rerun-if-changed=build/verification_key.json");
    println!("cargo:rerun-if-changed=build/compressed_account_merkle_proof_js");

    // Generate the verifying key Rust file from the JSON
    let vk_json_path = "./build/verification_key.json";
    let output_dir = "./src";
    let output_file = "verifying_key.rs";

    if std::path::Path::new(vk_json_path).exists() {
        generate_vk_file(vk_json_path, output_dir, output_file)
            .expect("Failed to generate verifying key Rust file");
        println!("cargo:warning=Generated verifying_key.rs from verification_key.json");
    } else {
        println!("cargo:warning=Verification key JSON not found. Run './scripts/setup.sh' first.");
    }

    // Transpile the WebAssembly witness generators for Circom circuits
    let witness_wasm_dir = "./build/compressed_account_merkle_proof_js";
    if std::path::Path::new(witness_wasm_dir).exists() {
        rust_witness::transpile::transpile_wasm(witness_wasm_dir.to_string());
        println!("cargo:warning=Transpiled witness generator");
    } else {
        println!("cargo:warning=Witness WASM not found. Run './scripts/setup.sh' first.");
    }
}
