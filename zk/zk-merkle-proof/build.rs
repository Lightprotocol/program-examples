use groth16_solana::vk_parser::generate_vk_file;
use rust_witness::transpile::transpile_wasm;

fn main() {
    println!("cargo:rerun-if-changed=build/verification_key.json");
    println!("cargo:rerun-if-changed=build/merkle_proof_js");

    // Generate the verifying key Rust file from the JSON
    let vk_json_path = "./build/verification_key.json";
    let output_dir = "./src";
    let output_file = "verifying_key.rs";

    if std::path::Path::new(vk_json_path).exists() {
        generate_vk_file(vk_json_path, output_dir, output_file)
            .expect("Failed to generate verifying key Rust file");
    } else {
        println!("cargo:warning=Verification key JSON not found. Run './scripts/setup.sh' first.");
    }

    // Only transpile witness generators for non-Solana targets
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("sbf") && !target.contains("solana") {
        // CARGO_MANIFEST_DIR gives the absolute path to the directory containing Cargo.toml
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
        let witness_dir = format!("{}/build/merkle_proof_js", manifest_dir);
        if std::path::Path::new(&witness_dir).exists() {
            transpile_wasm(witness_dir);
        } else {
            println!(
                "cargo:warning=Witness WASM not found at {}. Run './scripts/setup.sh' first.",
                witness_dir
            );
        }
    }
}
