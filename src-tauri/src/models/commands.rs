use serde::Deserialize;

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
