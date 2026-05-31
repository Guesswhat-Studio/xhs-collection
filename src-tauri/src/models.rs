use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalProfileSummary {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) source: Option<String>,
    pub(crate) source_account_id: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) db_path: String,
    pub(crate) media_dir: String,
    pub(crate) is_active: bool,
    pub(crate) session_status: String,
    pub(crate) last_opened_at: Option<String>,
    pub(crate) last_sync_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalProfile {
    pub(crate) id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) source_account_id: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) db_relative_path: String,
    pub(crate) media_relative_path: String,
    pub(crate) is_active: bool,
    pub(crate) session_status: String,
    pub(crate) last_opened_at: Option<String>,
    pub(crate) last_sync_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryOverview {
    pub(crate) app_data_dir: String,
    pub(crate) db_path: String,
    pub(crate) media_dir: String,
    pub(crate) notes_count: i64,
    pub(crate) media_count: i64,
    pub(crate) content_coverage: LibraryContentCoverage,
    pub(crate) storage_root_id: String,
    pub(crate) active_profile: LocalProfileSummary,
    pub(crate) profiles: Vec<LocalProfileSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryContentCoverage {
    pub(crate) total_notes: i64,
    pub(crate) detail_notes: i64,
    pub(crate) tagged_notes: i64,
    pub(crate) media_notes: i64,
    pub(crate) missing_detail_notes: i64,
    pub(crate) missing_tag_notes: i64,
    pub(crate) unique_tags: i64,
}

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaAsset {
    pub(crate) id: String,
    pub(crate) note_id: String,
    pub(crate) media_type: String,
    pub(crate) download_status: String,
    pub(crate) original_url: Option<String>,
    pub(crate) relative_path: Option<String>,
    pub(crate) mime_type: Option<String>,
    pub(crate) size_bytes: Option<i64>,
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteSummary {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) source_note_id: String,
    pub(crate) source_url: String,
    pub(crate) title: String,
    pub(crate) excerpt: String,
    pub(crate) content: String,
    pub(crate) author_name: String,
    pub(crate) cover_url: Option<String>,
    pub(crate) note_type: String,
    pub(crate) published_at: Option<String>,
    pub(crate) collected_at: Option<String>,
    pub(crate) favorite_order: Option<i64>,
    pub(crate) last_synced_at: String,
    pub(crate) last_seen_at: Option<String>,
    pub(crate) remote_missing_at: Option<String>,
    pub(crate) remote_status: String,
    pub(crate) unavailable_reason: Option<String>,
    pub(crate) status: String,
    pub(crate) category_name: Option<String>,
    pub(crate) user_note: String,
    pub(crate) tags: Vec<String>,
    pub(crate) media: Vec<MediaAsset>,
}

#[derive(Debug)]
pub(crate) struct BaseNote {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) source_note_id: String,
    pub(crate) source_url: String,
    pub(crate) title: String,
    pub(crate) excerpt: String,
    pub(crate) content: String,
    pub(crate) author_name: String,
    pub(crate) cover_url: Option<String>,
    pub(crate) note_type: String,
    pub(crate) published_at: Option<String>,
    pub(crate) collected_at: Option<String>,
    pub(crate) favorite_order: Option<i64>,
    pub(crate) last_synced_at: String,
    pub(crate) last_seen_at: Option<String>,
    pub(crate) remote_missing_at: Option<String>,
    pub(crate) remote_status: String,
    pub(crate) unavailable_reason: Option<String>,
    pub(crate) status: String,
    pub(crate) category_name: Option<String>,
    pub(crate) user_note: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ImportSummary {
    pub(crate) inserted: usize,
    pub(crate) updated: usize,
    pub(crate) skipped: usize,
    #[serde(skip_serializing)]
    pub(crate) note_ids: Vec<String>,
}

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

#[derive(Debug, Deserialize)]
pub(crate) struct StatusUpdateInput {
    #[serde(rename = "noteId")]
    pub(crate) note_id: String,
    pub(crate) status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteMetadataUpdateInput {
    pub(crate) note_id: String,
    pub(crate) status: Option<String>,
    pub(crate) category_name: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) user_note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchNoteMetadataUpdateInput {
    pub(crate) note_ids: Vec<String>,
    pub(crate) status: Option<String>,
    pub(crate) category_name: Option<String>,
    pub(crate) add_tags: Option<Vec<String>>,
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSettings {
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) has_api_key: bool,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i64,
    pub(crate) updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AiRuntimeConfig {
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) api_key: String,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSettingsInput {
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) api_key: Option<String>,
    pub(crate) clear_api_key: Option<bool>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSettingsTestResult {
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiPromptEditorItem {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) system: String,
    pub(crate) user: Option<String>,
    pub(crate) task: Option<String>,
    pub(crate) rules: Vec<String>,
    pub(crate) schema_kind: Option<String>,
    pub(crate) schema_text: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiPromptSettings {
    pub(crate) path: String,
    pub(crate) is_custom: bool,
    pub(crate) validation_error: Option<String>,
    pub(crate) prompts: Vec<AiPromptEditorItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiPromptSettingsInput {
    pub(crate) prompts: Vec<AiPromptEditorItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiClassifyInput {
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSplitCategoryInput {
    pub(crate) source_category_name: String,
    pub(crate) target_category_name: String,
    pub(crate) query: String,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiTagGroupInput {
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiTagMergeSuggestInput {
    pub(crate) limit: Option<usize>,
    pub(crate) use_ai: Option<bool>,
    pub(crate) min_confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagMergeGroupInput {
    pub(crate) canonical_tag: String,
    pub(crate) duplicate_tags: Vec<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagGovernanceApplyInput {
    pub(crate) remove_tags: Vec<String>,
    pub(crate) merge_groups: Vec<TagMergeGroupInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiAssignmentResult {
    pub(crate) note_id: String,
    pub(crate) title: String,
    pub(crate) category_name: String,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiClassificationResult {
    pub(crate) scanned: usize,
    pub(crate) updated: usize,
    pub(crate) created_categories: Vec<String>,
    pub(crate) assignments: Vec<AiAssignmentResult>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagSummary {
    pub(crate) name: String,
    pub(crate) count: i64,
    pub(crate) kind: String,
    pub(crate) group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiTagAssignmentResult {
    pub(crate) tag: String,
    pub(crate) group_name: String,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiTagGroupResult {
    pub(crate) scanned: usize,
    pub(crate) updated: usize,
    pub(crate) groups: Vec<String>,
    pub(crate) assignments: Vec<AiTagAssignmentResult>,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagGroupClearResult {
    pub(crate) scanned: usize,
    pub(crate) cleared: usize,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagCleanupIssue {
    pub(crate) tag: String,
    pub(crate) count: i64,
    pub(crate) issue_kind: String,
    pub(crate) action: String,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagMergeSuggestion {
    pub(crate) canonical_tag: String,
    pub(crate) duplicate_tags: Vec<String>,
    pub(crate) affected_notes: i64,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
    pub(crate) source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagGovernanceSuggestionResult {
    pub(crate) scanned: usize,
    pub(crate) cleanup_issues: Vec<TagCleanupIssue>,
    pub(crate) merge_groups: Vec<TagMergeSuggestion>,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TagGovernanceApplyResult {
    pub(crate) removed_tags: usize,
    pub(crate) merged_tags: usize,
    pub(crate) aliases_created: usize,
    pub(crate) affected_notes: usize,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AiNoteDigest {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) excerpt: String,
    pub(crate) content: String,
    pub(crate) author_name: String,
    pub(crate) category_name: Option<String>,
    pub(crate) tags: Vec<String>,
}

pub(crate) struct LibraryPaths {
    pub(crate) app_data_dir: PathBuf,
    pub(crate) db_path: PathBuf,
    pub(crate) media_dir: PathBuf,
    pub(crate) profile: LocalProfile,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryBackupResult {
    pub(crate) path: String,
    pub(crate) file_count: usize,
    pub(crate) size_bytes: i64,
    pub(crate) message: String,
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
