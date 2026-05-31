use serde::Serialize;

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
