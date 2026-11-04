use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let script = manifest.join("build-support/gen-test-certs.sh");
    println!("cargo:rerun-if-changed={}", script.display());

    // A release build must not need openssl or generate test material.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("fixtures");
    let status = Command::new("sh")
        .arg(&script)
        .arg(&out)
        .status()
        .expect("run gen-test-certs.sh");
    assert!(status.success(), "gen-test-certs.sh failed");
}
