// Copies pdfium.dll from the project root next to the built exe so the
// binary runs without manual dll handling (`cargo run`, release builds).

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dll = manifest_dir.join("pdfium.dll");
    println!("cargo:rerun-if-changed={}", dll.display());

    if !dll.exists() {
        println!(
            "cargo:warning=pdfium.dll not found in project root; \
             download pdfium-win-x64.tgz from github.com/bblanchon/pdfium-binaries"
        );
        return;
    }

    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out  ->  profile dir is ../../..
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if let Some(profile_dir) = out_dir.ancestors().nth(3) {
        let dest = profile_dir.join("pdfium.dll");
        if let Err(e) = std::fs::copy(&dll, &dest) {
            println!("cargo:warning=failed to copy pdfium.dll: {e}");
        }
    }
}
