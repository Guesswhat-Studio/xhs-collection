use serde::Serialize;

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
