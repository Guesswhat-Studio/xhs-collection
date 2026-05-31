use serde::Serialize;
use std::path::PathBuf;

use super::profile::{LocalProfile, LocalProfileSummary};

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

pub(crate) struct LibraryPaths {
    pub(crate) app_data_dir: PathBuf,
    pub(crate) db_path: PathBuf,
    pub(crate) media_dir: PathBuf,
    pub(crate) profile: LocalProfile,
}
