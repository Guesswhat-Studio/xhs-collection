use crate::models::{ClearLogFilesResult, LogFileInfo};
use crate::utils::display_path;
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager, Runtime};

const LOG_STEM_PREFIX: &str = "xhs-collection";
const LATEST_LOG_NAME: &str = "latest.log";
static CURRENT_LOG_STEM: OnceLock<String> = OnceLock::new();

pub(crate) fn current_log_stem() -> &'static str {
    CURRENT_LOG_STEM
        .get_or_init(|| format!("{LOG_STEM_PREFIX}-{}", Local::now().format("%Y%m%d-%H%M%S")))
        .as_str()
}

pub(crate) fn current_log_name() -> String {
    format!("{}.log", current_log_stem())
}

#[tauri::command]
pub(crate) fn get_log_file_info(app: AppHandle) -> Result<LogFileInfo, String> {
    log_file_info(&app)
}

#[tauri::command]
pub(crate) fn clear_log_files(app: AppHandle) -> Result<ClearLogFilesResult, String> {
    let log_dir = resolve_log_dir(&app)?;
    fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;

    let current_name = current_log_name();
    let mut deleted = 0usize;
    let mut kept = 0usize;
    let mut failed = Vec::new();

    for entry in fs::read_dir(&log_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !is_log_file(&path) {
            continue;
        }

        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            kept += 1;
            continue;
        };

        if name == current_name {
            kept += 1;
            continue;
        }

        match fs::remove_file(&path) {
            Ok(()) => deleted += 1,
            Err(error) => failed.push(format!("{name}: {error}")),
        }
    }

    let message = if failed.is_empty() {
        format!("已清理 {deleted} 个历史日志，当前会话日志已保留。")
    } else {
        format!(
            "已清理 {deleted} 个历史日志，{} 个文件清理失败。",
            failed.len()
        )
    };

    Ok(ClearLogFilesResult {
        deleted,
        kept,
        failed,
        message,
        info: log_file_info(&app)?,
    })
}

pub(crate) fn copy_current_log_to_latest<R: Runtime>(app: &AppHandle<R>) {
    log::logger().flush();
    if let Err(error) = copy_current_log_to_latest_inner(app) {
        log::warn!("latest_log_update_failed {error}");
    }
}

fn copy_current_log_to_latest_inner<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let log_dir = resolve_log_dir(app)?;
    fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;

    let current = current_log_path(&log_dir);
    if !current.exists() {
        return Ok(());
    }

    fs::copy(&current, latest_log_path(&log_dir)).map_err(|error| error.to_string())?;
    Ok(())
}

fn log_file_info<R: Runtime>(app: &AppHandle<R>) -> Result<LogFileInfo, String> {
    let log_dir = resolve_log_dir(app)?;
    fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;

    let current = current_log_path(&log_dir);
    let latest = latest_log_path(&log_dir);

    Ok(LogFileInfo {
        log_dir: display_path(log_dir),
        current_log_path: display_path(current.clone()),
        latest_log_path: display_path(latest.clone()),
        current_log_name: current_log_name(),
        latest_log_name: LATEST_LOG_NAME.to_string(),
        current_log_exists: current.exists(),
        latest_log_exists: latest.exists(),
    })
}

fn resolve_log_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path().app_log_dir().map_err(|error| error.to_string())
}

fn current_log_path(log_dir: &Path) -> PathBuf {
    log_dir.join(current_log_name())
}

fn latest_log_path(log_dir: &Path) -> PathBuf {
    log_dir.join(LATEST_LOG_NAME)
}

fn is_log_file(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|ext| ext == "log")
}
