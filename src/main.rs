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
    install_panic_hook();
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
    gui_log(&format!("gui start (v{})", env!("CARGO_PKG_VERSION")));

    let mut engine = match pdf::PdfEngine::new() {
        Ok(e) => e,
        Err(e) => {
            let msg = format!("{e:#}");
            gui_log(&format!("pdfium init failed: {msg}"));
            rfd::MessageDialog::new()
                .set_title("hema pdfTool")
                .set_description(msg)
                .set_level(rfd::MessageLevel::Error)
                .show();
            return 1;
        }
    };

    // Try the default renderer first (wgpu when both features are enabled),
    // then the other one. wgpu fails on machines without a usable GPU/driver
    // (VMs, remote desktop, old drivers); glow only needs basic OpenGL.
    let renderers = match eframe::Renderer::default() {
        eframe::Renderer::Wgpu => [eframe::Renderer::Wgpu, eframe::Renderer::Glow],
        eframe::Renderer::Glow => [eframe::Renderer::Glow, eframe::Renderer::Wgpu],
    };

    let mut failures = Vec::new();
    for renderer in renderers {
        gui_log(&format!("trying renderer {renderer}"));
        // run_native can panic (not just error) during GPU init.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_eframe(engine, renderer)
        }));
        match result {
            Ok(Ok(())) => return 0,
            Ok(Err(e)) => failures.push(format!("{renderer}: {e}")),
            Err(_) => failures.push(format!("{renderer}: panicked during startup")),
        }
        gui_log(&format!("renderer failed: {}", failures.last().unwrap()));
        match pdf::PdfEngine::new() {
            Ok(e) => engine = e,
            Err(e) => {
                gui_log(&format!("pdfium re-init failed: {e:#}"));
                break;
            }
        }
    }

    let msg = format!(
        "Could not start the application window.\n\n{}\n\nDetails: %APPDATA%\\hema_pdfTool\\gui.log",
        failures.join("\n")
    );
    gui_log(&format!("all renderers failed: {}", failures.join(" | ")));
    rfd::MessageDialog::new()
        .set_title("hema pdfTool")
        .set_description(msg)
        .set_level(rfd::MessageLevel::Error)
        .show();
    1
}

fn run_eframe(engine: pdf::PdfEngine, renderer: eframe::Renderer) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        renderer,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "hema pdfTool",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::PdfToolApp::new(engine, &cc.egui_ctx)))
        }),
    )
}

/// Logs panics to gui.log. A windows-subsystem exe has no console, so
/// without this a crash (e.g. during GPU init) is completely invisible.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        gui_log(&format!("panic: {info}"));
    }));
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
    write_log(settings::log_path(), msg);
}

fn gui_log(msg: &str) {
    write_log(settings::gui_log_path(), msg);
}

fn write_log(path: Option<PathBuf>, msg: &str) {
    if let Some(path) = path {
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
