use groth16_solana::vk_parser::generate_vk_file;

fn main() {
    println!("cargo:rerun-if-changed=build/nullifier_1_verification_key.json");
    println!("cargo:rerun-if-changed=build/nullifier_4_verification_key.json");
    println!("cargo:rerun-if-changed=build/nullifier_1_js");
    println!("cargo:rerun-if-changed=build/nullifier_4_js");

    // Single nullifier verifying key
    if std::path::Path::new("./build/nullifier_1_verification_key.json").exists() {
        generate_vk_file(
            "./build/nullifier_1_verification_key.json",
            "./src",
            "nullifier_1.rs",
        )
        .expect("Failed to generate nullifier_1.rs");
    } else {
        println!("cargo:warning=nullifier_1_verification_key.json not found. Run './scripts/setup.sh'");
    }

    // Batch nullifier verifying key
    if std::path::Path::new("./build/nullifier_4_verification_key.json").exists() {
        generate_vk_file(
            "./build/nullifier_4_verification_key.json",
            "./src",
            "nullifier_batch_4.rs",
        )
        .expect("Failed to generate nullifier_batch_4.rs");
    } else {
        println!("cargo:warning=nullifier_4_verification_key.json not found. Run './scripts/setup.sh'");
    }

    // Transpile witness generators for non-Solana targets
    // Each circuit gets its own named library so tests can link separately
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("sbf") && !target.contains("solana") {
        let out_dir = std::env::var("OUT_DIR").unwrap();

        // Transpile single nullifier circuit
        if std::path::Path::new("./build/nullifier_1_js").exists() {
            rust_witness::transpile::transpile_wasm("./build/nullifier_1_js".to_string());
            // Rename libcircuit.a → libcircuit_single.a
            let src = format!("{}/libcircuit.a", out_dir);
            let dst = format!("{}/libcircuit_single.a", out_dir);
            if std::path::Path::new(&src).exists() {
                std::fs::copy(&src, &dst).expect("Failed to copy libcircuit_single.a");
            }
        }

        // Transpile batch nullifier circuit
        if std::path::Path::new("./build/nullifier_4_js").exists() {
            rust_witness::transpile::transpile_wasm("./build/nullifier_4_js".to_string());
            // Rename libcircuit.a → libcircuit_batch.a
            let src = format!("{}/libcircuit.a", out_dir);
            let dst = format!("{}/libcircuit_batch.a", out_dir);
            if std::path::Path::new(&src).exists() {
                std::fs::copy(&src, &dst).expect("Failed to copy libcircuit_batch.a");
            }
        }

        // Emit link search path (rust_witness already does this, but be explicit)
        println!("cargo:rustc-link-search=native={}", out_dir);
    }
}
