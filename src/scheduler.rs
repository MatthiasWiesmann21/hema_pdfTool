//! Windows Task Scheduler integration via schtasks.exe.
//!
//! Registers a task that runs this exe in headless merge mode:
//! `hema_pdf_tool.exe --merge-folder <dir> --output <file>`

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

pub const TASK_NAME: &str = "HemaPdfTool-AutoMerge";

const WEEKDAYS: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

#[derive(Debug, Clone, PartialEq)]
pub enum Frequency {
    Daily,
    /// Index into WEEKDAYS, 0 = Monday.
    Weekly(usize),
    OnLogon,
}

pub struct ScheduleConfig {
    pub input_dir: String,
    pub output: String,
    pub time: String, // "HH:MM" 24h
    pub frequency: Frequency,
}

/// (Re)creates the scheduled task. Returns schtasks output for display.
pub fn install_task(cfg: &ScheduleConfig) -> Result<String> {
    let exe = std::env::current_exe().context("could not locate exe")?;
    let run = format!(
        "\"{}\" --merge-folder \"{}\" --output \"{}\"",
        exe.display(),
        cfg.input_dir,
        cfg.output
    );

    let mut args = vec![
        "/Create".to_string(),
        "/F".to_string(),
        "/TN".to_string(),
        TASK_NAME.to_string(),
        "/TR".to_string(),
        run,
    ];

    match &cfg.frequency {
        Frequency::Daily => {
            validate_time(&cfg.time)?;
            args.extend(["/SC".into(), "DAILY".into(), "/ST".into(), cfg.time.clone()]);
        }
        Frequency::Weekly(day) => {
            validate_time(&cfg.time)?;
            let day = WEEKDAYS.get(*day).copied().unwrap_or("MON");
            args.extend([
                "/SC".into(),
                "WEEKLY".into(),
                "/D".into(),
                day.into(),
                "/ST".into(),
                cfg.time.clone(),
            ]);
        }
        Frequency::OnLogon => {
            args.extend(["/SC".into(), "ONLOGON".into()]);
        }
    }

    run_schtasks(&args)
}

pub fn remove_task() -> Result<String> {
    run_schtasks(&[
        "/Delete".to_string(),
        "/F".to_string(),
        "/TN".to_string(),
        TASK_NAME.to_string(),
    ])
}

/// Returns the task's listing if it exists, `None` if no such task.
pub fn query_task() -> Option<String> {
    let output = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME, "/FO", "LIST", "/V"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Immediately runs the task (handy for testing from the GUI).
pub fn run_task_now() -> Result<String> {
    run_schtasks(&["/Run".to_string(), "/TN".to_string(), TASK_NAME.to_string()])
}

fn run_schtasks(args: &[String]) -> Result<String> {
    let output = Command::new("schtasks")
        .args(args)
        .output()
        .context("failed to run schtasks.exe")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        anyhow::bail!("schtasks failed: {} {}", stdout.trim(), stderr.trim())
    }
}

fn validate_time(time: &str) -> Result<()> {
    let (h, m) = time
        .split_once(':')
        .context("time must be in HH:MM format")?;
    let h: u32 = h.parse().context("invalid hour")?;
    let m: u32 = m.parse().context("invalid minute")?;
    anyhow::ensure!(h < 24 && m < 60, "time must be in HH:MM (24h) format");
    Ok(())
}

/// Checks the paths and file extension before creating the task.
pub fn validate_config(cfg: &ScheduleConfig) -> Result<()> {
    // The paths are embedded in the task's /TR command line inside quotes —
    // a double quote would break out of it and let the text inject extra
    // commands into the scheduled task.
    for (what, s) in [
        ("input folder", cfg.input_dir.as_str()),
        ("output file", cfg.output.as_str()),
    ] {
        anyhow::ensure!(
            !s.contains('"') && !s.chars().any(char::is_control),
            "{what} contains characters that are not allowed"
        );
    }
    anyhow::ensure!(
        Path::new(&cfg.input_dir).is_dir(),
        "input folder does not exist"
    );
    anyhow::ensure!(
        cfg.output.to_lowercase().ends_with(".pdf"),
        "output must be a .pdf file"
    );
    // The scheduled task has no meaningful working directory, so a bare
    // file name would land somewhere unexpected — require a real folder.
    let has_folder = Path::new(&cfg.output)
        .parent()
        .is_some_and(|p| !p.as_os_str().is_empty() && p.is_dir());
    anyhow::ensure!(has_folder, "output must include an existing folder");
    Ok(())
}
