use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XhsFavoriteSyncInput {
    pub(crate) max_count: Option<usize>,
    pub(crate) resume: Option<bool>,
    pub(crate) full_sync: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XhsAlbumSyncInput {
    pub(crate) max_albums: Option<usize>,
    pub(crate) max_notes_per_album: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XhsFavoriteSyncResult {
    pub(crate) scanned: usize,
    pub(crate) fetched: usize,
    pub(crate) inserted: usize,
    pub(crate) updated: usize,
    pub(crate) skipped: usize,
    pub(crate) existing_skipped: usize,
    pub(crate) remote_missing: usize,
    pub(crate) remote_display_count: Option<usize>,
    pub(crate) remote_unreturned_count: Option<usize>,
    pub(crate) limit_reached: bool,
    pub(crate) full_sync: bool,
    pub(crate) details_updated: usize,
    pub(crate) details_failed: usize,
    pub(crate) covers_downloaded: usize,
    pub(crate) covers_failed: usize,
    pub(crate) media_downloaded: usize,
    pub(crate) media_failed: usize,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XhsAlbumSyncResult {
    pub(crate) albums_scanned: usize,
    pub(crate) albums_updated: usize,
    pub(crate) notes_scanned: usize,
    pub(crate) notes_linked: usize,
    pub(crate) notes_inserted: usize,
    pub(crate) notes_updated: usize,
    pub(crate) duplicate_notes: usize,
    pub(crate) skipped: usize,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XhsSyncProgress {
    pub(crate) phase: String,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) planned: usize,
    pub(crate) scanned: usize,
    pub(crate) fetched: usize,
    pub(crate) to_sync: Option<usize>,
    pub(crate) written: usize,
    pub(crate) inserted: usize,
    pub(crate) updated: usize,
    pub(crate) skipped: usize,
    pub(crate) existing_skipped: usize,
    pub(crate) progress: u8,
    pub(crate) indeterminate: bool,
}

pub(crate) struct XhsFavoriteCollectResult {
    pub(crate) notes: Vec<Value>,
    pub(crate) seen_note_ids: HashSet<String>,
    pub(crate) favorite_positions: Vec<(String, usize)>,
    pub(crate) scanned: usize,
    pub(crate) existing_skipped: usize,
    pub(crate) limit_reached: bool,
    pub(crate) reached_end: bool,
    pub(crate) stopped_reason: String,
    pub(crate) remote_display_count: Option<usize>,
    pub(crate) first_source_note_id: Option<String>,
    pub(crate) last_source_note_id: Option<String>,
}

#[derive(Debug)]
pub(crate) struct XhsSyncCheckpoint {
    pub(crate) anchor_source_note_id: Option<String>,
    pub(crate) reached_end: bool,
    pub(crate) scanned_count: usize,
    pub(crate) remote_display_count: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct XhsAccountInfo {
    pub(crate) user_id: Option<String>,
    pub(crate) nickname: Option<String>,
    pub(crate) avatar_url: Option<String>,
}

#[derive(Debug)]
pub(crate) struct StoredXhsSession {
    pub(crate) source_account_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) session_key_id: Option<String>,
    pub(crate) session_storage: String,
    pub(crate) session_cookie: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct XhsSessionTestResult {
    pub(crate) ok: bool,
    pub(crate) status_code: u16,
    pub(crate) final_url: String,
    pub(crate) page_title: Option<String>,
    pub(crate) account_hint: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) account_name: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) cookie_keys: Vec<String>,
    pub(crate) checked_at: String,
    pub(crate) message: String,
}

#[derive(Debug)]
pub(crate) struct NoteFetchTarget {
    pub(crate) id: String,
    pub(crate) source_note_id: String,
    pub(crate) source_url: String,
}

#[derive(Debug, Clone)]
pub(crate) struct MediaDownloadTarget {
    pub(crate) id: String,
    pub(crate) note_id: String,
    pub(crate) source_asset_id: Option<String>,
    pub(crate) note_source_note_id: String,
    pub(crate) media_type: String,
    pub(crate) original_url: String,
    pub(crate) mime_type: Option<String>,
}

#[derive(Debug)]
pub(crate) struct VideoStreamCandidate {
    pub(crate) url: String,
    pub(crate) codec: String,
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) size_bytes: Option<i64>,
}

#[derive(Debug)]
pub(crate) enum XhsDetailFetchError {
    Gone(u16),
    NeedsVerification(String),
    Other(String),
}

#[derive(Debug)]
pub(crate) struct ExtractedMediaAsset {
    pub(crate) source_asset_id: String,
    pub(crate) media_type: String,
    pub(crate) original_url: String,
    pub(crate) mime_type: Option<String>,
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) size_bytes: Option<i64>,
}
