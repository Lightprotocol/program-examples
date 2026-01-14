use groth16_solana::vk_parser::generate_vk_file;

fn main() {
    // Build artifacts are at workspace root
    let build_dir = "../../build";

    println!("cargo:rerun-if-changed={}/nullifier_1_verification_key.json", build_dir);
    println!("cargo:rerun-if-changed={}/nullifier_4_verification_key.json", build_dir);
    println!("cargo:rerun-if-changed={}/nullifier_1_js", build_dir);
    println!("cargo:rerun-if-changed={}/nullifier_4_js", build_dir);

    // Single nullifier verifying key
    let vk1_path = format!("{}/nullifier_1_verification_key.json", build_dir);
    if std::path::Path::new(&vk1_path).exists() {
        generate_vk_file(&vk1_path, "./src", "nullifier_1.rs")
            .expect("Failed to generate nullifier_1.rs");
    } else {
        println!("cargo:warning=nullifier_1_verification_key.json not found. Run './scripts/setup.sh'");
    }

    // Batch nullifier verifying key
    let vk4_path = format!("{}/nullifier_4_verification_key.json", build_dir);
    if std::path::Path::new(&vk4_path).exists() {
        generate_vk_file(&vk4_path, "./src", "nullifier_batch_4.rs")
            .expect("Failed to generate nullifier_batch_4.rs");
    } else {
        println!("cargo:warning=nullifier_4_verification_key.json not found. Run './scripts/setup.sh'");
    }

    // Transpile witness generators for non-Solana targets
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("sbf") && !target.contains("solana") {
        let out_dir = std::env::var("OUT_DIR").unwrap();

        // Transpile single nullifier circuit
        let js1_path = format!("{}/nullifier_1_js", build_dir);
        if std::path::Path::new(&js1_path).exists() {
            rust_witness::transpile::transpile_wasm(js1_path);
            let src = format!("{}/libcircuit.a", out_dir);
            let dst = format!("{}/libcircuit_single.a", out_dir);
            if std::path::Path::new(&src).exists() {
                std::fs::copy(&src, &dst).expect("Failed to copy libcircuit_single.a");
            }
        }

        // Transpile batch nullifier circuit
        let js4_path = format!("{}/nullifier_4_js", build_dir);
        if std::path::Path::new(&js4_path).exists() {
            rust_witness::transpile::transpile_wasm(js4_path);
            let src = format!("{}/libcircuit.a", out_dir);
            let dst = format!("{}/libcircuit_batch.a", out_dir);
            if std::path::Path::new(&src).exists() {
                std::fs::copy(&src, &dst).expect("Failed to copy libcircuit_batch.a");
            }
        }

        println!("cargo:rustc-link-search=native={}", out_dir);
    }
}
