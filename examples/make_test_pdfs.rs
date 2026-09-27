//! Generates test PDFs in ./testdata — pages have distinct sizes so page
//! order/rotation can be verified in merged output (page 1 = smallest, etc.).
//!
//! Usage: cargo run --example make_test_pdfs

use pdfium_render::prelude::*;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let out_dir = Path::new("testdata");
    std::fs::create_dir_all(out_dir)?;

    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("."))
            .or_else(|_| Pdfium::bind_to_system_library())?,
    );

    // two files, 3 and 2 pages; each page a distinct size: 100..~600 pt squares
    for (file_i, page_count) in [(1usize, 3u16), (2, 2)] {
        let mut doc = pdfium.create_new_pdf()?;
        for i in 0..page_count {
            let side = 300.0 + (file_i * 10 + i as usize) as f64 * 50.0;
            doc.pages_mut()
                .create_page_at_end(PdfPagePaperSize::new_custom(
                    PdfPoints::new(side as f32),
                    PdfPoints::new(side as f32),
                ))?;
        }
        let path = out_dir.join(format!("test{file_i}.pdf"));
        doc.save_to_file(&path)?;
        println!("wrote {} ({page_count} pages)", path.display());
    }

    Ok(())
}
