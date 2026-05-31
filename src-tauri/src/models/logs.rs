use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogFileInfo {
    pub(crate) log_dir: String,
    pub(crate) current_log_path: String,
    pub(crate) latest_log_path: String,
    pub(crate) current_log_name: String,
    pub(crate) latest_log_name: String,
    pub(crate) current_log_exists: bool,
    pub(crate) latest_log_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClearLogFilesResult {
    pub(crate) deleted: usize,
    pub(crate) kept: usize,
    pub(crate) failed: Vec<String>,
    pub(crate) message: String,
    pub(crate) info: LogFileInfo,
}
