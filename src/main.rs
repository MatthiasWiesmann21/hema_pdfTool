#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use hema_pdf_tool::pdf::PageOp;
use hema_pdf_tool::{app, pdf, settings};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Drag & drop PDF page organizer and merger.
///
/// Without arguments opens the GUI. With --merge-folder + --output runs
/// headless and merges every PDF in the folder into one file.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Headless mode: merge all PDFs found in this folder (alphabetical order).
    #[arg(long, value_name = "DIR")]
    merge_folder: Option<PathBuf>,

    /// Output PDF path for headless merge.
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let code = match (cli.merge_folder, cli.output) {
        (Some(dir), Some(out)) => headless_merge(&dir, &out),
        (None, None) => run_gui(),
        _ => {
            eprintln!("--merge-folder and --output must be used together");
            2
        }
    };
    std::process::exit(code);
}

fn run_gui() -> i32 {
    let engine = match pdf::PdfEngine::new() {
        Ok(e) => e,
        Err(e) => {
            rfd::MessageDialog::new()
                .set_title("hema pdfTool")
                .set_description(format!("{e:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
            return 1;
        }
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    match eframe::run_native(
        "hema pdfTool",
        options,
        Box::new(|cc| Ok(Box::new(app::PdfToolApp::new(engine, &cc.egui_ctx)))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// Merges every *.pdf in `dir` (sorted by file name) into `out`.
/// Logs to %APPDATA%\hema_pdfTool\merge.log and stdout (console builds).
fn headless_merge(dir: &Path, out: &Path) -> i32 {
    match headless_merge_inner(dir, out) {
        Ok((files, pages)) => {
            let msg = format!(
                "merged {} file(s), {} page(s) -> {}",
                files,
                pages,
                out.display()
            );
            log(&msg);
            println!("{msg}");
            0
        }
        Err(e) => {
            let msg = format!("merge failed ({} -> {}): {e:#}", dir.display(), out.display());
            log(&msg);
            eprintln!("{msg}");
            1
        }
    }
}

fn headless_merge_inner(dir: &Path, out: &Path) -> anyhow::Result<(usize, usize)> {
    anyhow::ensure!(dir.is_dir(), "input folder not found: {}", dir.display());

    let engine = pdf::PdfEngine::new()?;

    let mut pdfs: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .collect();
    pdfs.sort();
    anyhow::ensure!(!pdfs.is_empty(), "no PDF files in {}", dir.display());

    // Don't merge the output file into itself.
    let out_canon = std::fs::canonicalize(out).ok();
    pdfs.retain(|p| std::fs::canonicalize(p).ok() != out_canon);
    anyhow::ensure!(!pdfs.is_empty(), "no input PDFs after excluding output");

    let mut ops = Vec::new();
    let mut skipped = Vec::new();
    for path in &pdfs {
        match engine.page_count(path) {
            Ok(n) => {
                for i in 0..n {
                    ops.push(PageOp {
                        source: path.clone(),
                        page_index: i,
                        rotation_delta: 0,
                    });
                }
            }
            Err(e) => skipped.push(format!("{}: {e:#}", path.display())),
        }
    }
    anyhow::ensure!(!ops.is_empty(), "no readable PDF pages found");

    let file_count = pdfs.len() - skipped.len();
    for s in &skipped {
        log(&format!("skipped {s}"));
    }

    engine.export(&ops, out)?;
    Ok((file_count, ops.len()))
}

fn log(msg: &str) {
    if let Some(path) = settings::log_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(f, "[{secs}] {msg}");
        }
    }
}
