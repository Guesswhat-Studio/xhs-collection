use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportLibraryInput {
    pub(crate) format: String,
    pub(crate) include_media: Option<bool>,
    pub(crate) include_notes: Option<bool>,
    pub(crate) only_reviewed: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportLibraryResult {
    pub(crate) path: String,
    pub(crate) format: String,
    pub(crate) note_count: usize,
    pub(crate) media_count: usize,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryBackupResult {
    pub(crate) path: String,
    pub(crate) file_count: usize,
    pub(crate) size_bytes: i64,
    pub(crate) message: String,
}
