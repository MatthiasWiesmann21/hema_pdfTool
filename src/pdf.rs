//! Thin wrapper around pdfium-render: thumbnails, page copy, rotation, export.

use anyhow::{Context, Result};
use pdfium_render::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One page in the output queue: which file it comes from, which page, and how
/// many extra quarter-turns (clockwise) to apply when exporting.
#[derive(Debug, Clone)]
pub struct PageOp {
    pub source: PathBuf,
    pub page_index: i32,
    /// Clockwise quarter-turns to add on top of the page's intrinsic rotation (0..=3).
    pub rotation_delta: u8,
}

/// Raw RGBA pixels of a rendered page thumbnail.
pub struct Thumbnail {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

pub struct PdfEngine {
    pdfium: Pdfium,
}

impl PdfEngine {
    /// Binds `pdfium.dll` located next to the running exe (or on the library path).
    pub fn new() -> Result<Self> {
        let exe_dir = std::env::current_exe()
            .context("could not locate exe directory")?
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            &exe_dir,
        ))
        .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(".")))
        .or_else(|_| Pdfium::bind_to_system_library())
        .context("could not load pdfium.dll (expected next to hema_pdf_tool.exe)")?;

        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }

    pub fn page_count(&self, path: &Path) -> Result<i32> {
        let doc = self
            .pdfium
            .load_pdf_from_file(path, None)
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(doc.pages().len())
    }

    /// Renders every page of `path` as a thumbnail of roughly `target_width` pixels wide.
    pub fn render_thumbnails(&self, path: &Path, target_width: i32) -> Result<Vec<Thumbnail>> {
        let doc = self
            .pdfium
            .load_pdf_from_file(path, None)
            .with_context(|| format!("failed to open {}", path.display()))?;

        let config = PdfRenderConfig::new().set_target_width(target_width);
        let mut thumbs = Vec::with_capacity(doc.pages().len() as usize);

        for index in 0..doc.pages().len() {
            let page = doc
                .pages()
                .get(index)
                .with_context(|| format!("page {index} not found in {}", path.display()))?;
            let bitmap = page
                .render_with_config(&config)
                .with_context(|| format!("failed to render page {index} of {}", path.display()))?;
            thumbs.push(Thumbnail {
                width: bitmap.width() as usize,
                height: bitmap.height() as usize,
                rgba: bitmap.as_rgba_bytes(),
            });
        }

        Ok(thumbs)
    }

    /// Builds a new PDF at `out` containing `ops` pages in the given order,
    /// applying each page's rotation delta.
    pub fn export(&self, ops: &[PageOp], out: &Path) -> Result<()> {
        anyhow::ensure!(!ops.is_empty(), "no pages to export");

        // Refuse to overwrite one of the input files — pages are copied out
        // of the source documents, so truncating a source mid-save would
        // corrupt both files.
        let out_key = std::fs::canonicalize(out).unwrap_or_else(|_| out.to_path_buf());
        for op in ops {
            let src_key = std::fs::canonicalize(&op.source).unwrap_or_else(|_| op.source.clone());
            anyhow::ensure!(
                src_key != out_key,
                "output file {} is also an input file",
                out.display()
            );
        }

        let mut new_doc = self.pdfium.create_new_pdf().context("create output pdf")?;
        let mut cache: HashMap<PathBuf, PdfDocument<'_>> = HashMap::new();

        for op in ops {
            let doc = match cache.entry(op.source.clone()) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => v.insert(
                    self.pdfium
                        .load_pdf_from_file(&op.source, None)
                        .with_context(|| format!("failed to open {}", op.source.display()))?,
                ),
            };

            let dest_index = new_doc.pages().len();
            new_doc
                .pages_mut()
                .copy_page_from_document(&*doc, op.page_index, dest_index)
                .with_context(|| {
                    format!(
                        "failed to copy page {} of {}",
                        op.page_index + 1,
                        op.source.display()
                    )
                })?;

            if op.rotation_delta % 4 != 0 {
                let mut page = new_doc
                    .pages()
                    .get(dest_index)
                    .context("copied page missing")?;
                let base = page
                    .rotation()
                    .unwrap_or(PdfPageRenderRotation::None);
                page.set_rotation(add_quarter_turns(base, op.rotation_delta));
            }
        }

        // Write to a sibling temp file first so a crash can't leave a
        // truncated PDF behind, then move it over the destination.
        let mut tmp_name = out.as_os_str().to_os_string();
        tmp_name.push(".tmp");
        let tmp = PathBuf::from(tmp_name);

        let result = new_doc
            .save_to_file(&tmp)
            .with_context(|| format!("failed to save {}", out.display()))
            .and_then(|()| replace_file(&tmp, out));

        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

/// Moves `tmp` over `dest` (Windows rename refuses to overwrite existing files).
fn replace_file(tmp: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_file(dest)
            .with_context(|| format!("could not replace {}", dest.display()))?;
    }
    std::fs::rename(tmp, dest).with_context(|| format!("could not write {}", dest.display()))
}

/// Adds `quarters` clockwise quarter-turns to a render rotation.
fn add_quarter_turns(rotation: PdfPageRenderRotation, quarters: u8) -> PdfPageRenderRotation {
    let base = match rotation {
        PdfPageRenderRotation::None => 0,
        PdfPageRenderRotation::Degrees90 => 1,
        PdfPageRenderRotation::Degrees180 => 2,
        PdfPageRenderRotation::Degrees270 => 3,
    };
    match (base + quarters) % 4 {
        0 => PdfPageRenderRotation::None,
        1 => PdfPageRenderRotation::Degrees90,
        2 => PdfPageRenderRotation::Degrees180,
        _ => PdfPageRenderRotation::Degrees270,
    }
}

/// Rotates an RGBA8 image by `quarters` clockwise quarter-turns.
/// Returns `(width, height, pixels)`.
pub fn rotate_rgba(rgba: &[u8], width: usize, height: usize, quarters: u8) -> (usize, usize, Vec<u8>) {
    match quarters % 4 {
        0 => (width, height, rgba.to_vec()),
        2 => {
            let mut out = vec![0u8; rgba.len()];
            for i in 0..width * height {
                let src = &rgba[i * 4..i * 4 + 4];
                let dst = (width * height - 1 - i) * 4;
                out[dst..dst + 4].copy_from_slice(src);
            }
            (width, height, out)
        }
        q => {
            let (nw, nh) = (height, width);
            let mut out = vec![0u8; rgba.len()];
            for y in 0..height {
                for x in 0..width {
                    let (dx, dy) = if q == 1 { (height - 1 - y, x) } else { (y, width - 1 - x) };
                    let dst = (dy * nw + dx) * 4;
                    out[dst..dst + 4].copy_from_slice(&rgba[(y * width + x) * 4..(y * width + x) * 4 + 4]);
                }
            }
            (nw, nh, out)
        }
    }
}
