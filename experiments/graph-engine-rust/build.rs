#[path = "tools/native_compat_verify.rs"]
mod verifier;

fn main() {
    let package = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = package.join("compat/windows.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    println!("cargo:rerun-if-env-changed=GRAPH_RUST_RUN_ARTIFACT_ROOT");
    let artifacts = std::path::PathBuf::from(
        std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT")
            .expect("GRAPH_RUST_RUN_ARTIFACT_ROOT is required"),
    );
    println!(
        "cargo:rerun-if-changed={}",
        package.join("compat/unicode-input-manifest.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        package.join("tools/generate_unicode.py").display()
    );
    let input = verifier::verify(&package, &artifacts).unwrap_or_else(|code| panic!("{code}"));
    println!("cargo:rerun-if-changed={}", input.input_path.display());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("unicode-tables.bin"), input.bytes)
        .expect("verified input copy failed");
    std::fs::write(
        output.join("compat_provenance.rs"),
        format!(
            "pub const MANIFEST_SHA256: &str = {:?};\npub const INPUT_SHA256: &str = {:?};\n",
            verifier::MANIFEST_SHA256,
            input.input_sha256
        ),
    )
    .expect("provenance write failed");
}
