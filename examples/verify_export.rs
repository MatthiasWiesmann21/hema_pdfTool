//! Verifies export correctness: page order (via distinct page sizes),
//! rotation flags, and page dropping. Run `make_test_pdfs` first.
//!
//! Usage: cargo run --example verify_export

use anyhow::{ensure, Result};
use hema_pdf_tool::pdf::{PageOp, PdfEngine};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};

fn page_sizes(path: &Path) -> Result<(Vec<f32>, Vec<i32>)> {
    // Reuses the already-initialized global Pdfium bindings (PdfEngine::new ran first).
    let pdfium = Pdfium::default();
    let doc = pdfium.load_pdf_from_file(path, None)?;
    let mut sizes = Vec::new();
    let mut rotations = Vec::new();
    for i in 0..doc.pages().len() {
        let page = doc.pages().get(i)?;
        sizes.push(page.width().value);
        let rot = match page.rotation().unwrap_or(PdfPageRenderRotation::None) {
            PdfPageRenderRotation::None => 0,
            PdfPageRenderRotation::Degrees90 => 1,
            PdfPageRenderRotation::Degrees180 => 2,
            PdfPageRenderRotation::Degrees270 => 3,
        };
        rotations.push(rot);
    }
    Ok((sizes, rotations))
}

fn main() -> Result<()> {
    let dir = PathBuf::from("testdata");
    let t1 = dir.join("test1.pdf"); // sizes 800, 850, 900
    let t2 = dir.join("test2.pdf"); // sizes 1300, 1350

    // 1. Reorder+delete+rotate export: [t2.p0 rotated CW, t1.p2, t1.p0]
    let engine = PdfEngine::new()?;
    let ops = vec![
        PageOp { source: t2.clone(), page_index: 0, rotation_delta: 1 },
        PageOp { source: t1.clone(), page_index: 2, rotation_delta: 0 },
        PageOp { source: t1.clone(), page_index: 0, rotation_delta: 0 },
    ];
    let out = dir.join("verify.pdf");
    engine.export(&ops, &out)?;

    let (sizes, rots) = page_sizes(&out)?;
    println!("verify.pdf sizes={sizes:?} rotations={rots:?}");
    ensure!(sizes == vec![1300.0, 900.0, 800.0], "page order/size mismatch");
    ensure!(rots == vec![1, 0, 0], "rotation mismatch");

    // 2. Headless merge result from earlier run: order [t1(800,850,900), t2(1300,1350)]
    let (sizes, _) = page_sizes(&dir.join("merged.pdf"))?;
    ensure!(
        sizes == vec![800.0, 850.0, 900.0, 1300.0, 1350.0],
        "merged.pdf order mismatch: {sizes:?}"
    );

    println!("ALL CHECKS PASSED");
    Ok(())
}
