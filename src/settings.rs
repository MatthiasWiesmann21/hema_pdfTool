//! Persisted settings in %APPDATA%\hema_pdfTool\settings.json (or platform config dir).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Folder to preselect in save dialogs.
    pub last_dir: Option<PathBuf>,
    /// Scheduler panel inputs.
    pub sched_input_dir: String,
    pub sched_output: String,
    pub sched_time: String,
    /// 0 = daily, 1 = weekly, 2 = at logon.
    pub sched_freq: usize,
    /// Weekday for weekly schedule: 0 = Monday .. 6 = Sunday.
    pub sched_weekday: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_dir: None,
            sched_input_dir: String::new(),
            sched_output: String::new(),
            sched_time: "18:00".into(),
            sched_freq: 0,
            sched_weekday: 0,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("hema_pdfTool").join("settings.json"))
}

pub fn load() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    if let Some(p) = settings_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = std::fs::write(p, json);
        }
    }
}

/// Path of the log file used by headless merge runs.
pub fn log_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("hema_pdfTool").join("merge.log"))
}
