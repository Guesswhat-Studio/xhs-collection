use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchJobInput {
    pub(crate) limit: Option<usize>,
    pub(crate) asset_id: Option<String>,
    pub(crate) note_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchJobResult {
    pub(crate) scanned: usize,
    pub(crate) updated: usize,
    pub(crate) downloaded: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchJobProgress {
    pub(crate) phase: String,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) planned: usize,
    pub(crate) scanned: usize,
    pub(crate) updated: usize,
    pub(crate) downloaded: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) progress: u8,
    pub(crate) indeterminate: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiJobProgress {
    pub(crate) task: String,
    pub(crate) phase: String,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) planned: usize,
    pub(crate) scanned: usize,
    pub(crate) updated: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) progress: u8,
    pub(crate) indeterminate: bool,
    pub(crate) error: Option<String>,
}
