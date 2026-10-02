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

/// Exit code of a pinned child when pdfium itself failed to load —
/// retrying another renderer won't help, so the launcher stops.
const EXIT_PDFIUM_FAILED: i32 = 3;

/// Env var that pins a child process to one renderer.
const RENDERER_ENV: &str = "HEMA_PDFTOOL_RENDERER";

fn run_gui() -> i32 {
    gui_log(&format!("gui start (v{})", env!("CARGO_PKG_VERSION")));

    match std::env::var_os(RENDERER_ENV) {
        // Pinned child: runs exactly one renderer. A hard crash in GPU/driver
        // code (no panic, no error — the process just dies) kills only this
        // child; the launcher then tries the next renderer.
        Some(name) => {
            let renderer = if name == "glow" {
                eframe::Renderer::Glow
            } else {
                eframe::Renderer::Wgpu
            };
            gui_log(&format!("child mode, renderer {renderer}"));
            run_gui_child(renderer)
        }
        None => run_gui_launcher(),
    }
}

/// Spawns the GUI once per renderer (last-working one first). This process
/// never touches GPU code, so driver crashes can't take it down.
fn run_gui_launcher() -> i32 {
    let order = match settings::load().renderer.as_deref() {
        Some("glow") => ["glow", "wgpu"],
        _ => ["wgpu", "glow"],
    };

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        // Can't respawn ourselves — run in-process as a last resort.
        Err(e) => {
            gui_log(&format!("current_exe failed ({e}), running wgpu in-process"));
            return run_gui_child(eframe::Renderer::Wgpu);
        }
    };

    let mut failures = Vec::new();
    for name in order {
        gui_log(&format!("launching renderer {name}"));
        match std::process::Command::new(&exe)
            .env(RENDERER_ENV, name)
            .status()
        {
            Ok(s) if s.success() => {
                remember_renderer(name);
                return 0;
            }
            Ok(s) if s.code() == Some(EXIT_PDFIUM_FAILED) => return 1,
            Ok(s) => failures.push(format!("{name}: {s}")),
            Err(e) => failures.push(format!("{name}: failed to launch ({e})")),
        }
        gui_log(&format!("renderer {name} failed: {}", failures.last().unwrap()));
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

fn remember_renderer(name: &str) {
    let mut s = settings::load();
    s.renderer = Some(name.to_string());
    settings::save(&s);
}

fn run_gui_child(renderer: eframe::Renderer) -> i32 {
    let engine = match pdf::PdfEngine::new() {
        Ok(e) => e,
        Err(e) => {
            let msg = format!("{e:#}");
            gui_log(&format!("pdfium init failed: {msg}"));
            rfd::MessageDialog::new()
                .set_title("hema pdfTool")
                .set_description(msg)
                .set_level(rfd::MessageLevel::Error)
                .show();
            return EXIT_PDFIUM_FAILED;
        }
    };

    gui_log(&format!("trying renderer {renderer}"));
    match run_eframe(engine, renderer) {
        Ok(()) => {
            gui_log("gui exited normally");
            0
        }
        Err(e) => {
            gui_log(&format!("renderer {renderer} failed: {e}"));
            1
        }
    }
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
            gui_log("window created");
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
