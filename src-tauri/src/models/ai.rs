use serde::{Deserialize, Serialize};

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
