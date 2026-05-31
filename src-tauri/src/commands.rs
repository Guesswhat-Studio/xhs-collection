use crate::models::*;
use crate::storage::{
    app_data_dir, ensure_storage_root, library_overview, normalize_id_list, normalize_tag_names,
    open_library, open_profile_registry, profile_summaries, read_local_profiles, read_notes,
    replace_user_tags_for_note, resolve_paths, set_active_local_profile, upsert_optional_category,
    upsert_tag_with_kind,
};
use crate::utils::display_path;
use chrono::Utc;
use rusqlite::params;
use serde_json::json;
use std::fs;
use tauri::AppHandle;
#[tauri::command]
pub(crate) fn get_library_overview(app: AppHandle) -> Result<LibraryOverview, String> {
    let (paths, conn) = open_library(&app)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
pub(crate) fn list_local_profiles(app: AppHandle) -> Result<Vec<LocalProfileSummary>, String> {
    let app_data_dir = app_data_dir(&app)?;
    let (_, conn) = open_profile_registry(&app)?;
    read_local_profiles(&conn)
        .map(|profiles| profile_summaries(&app_data_dir, profiles))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn switch_local_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<LibraryOverview, String> {
    let (_, conn) = open_profile_registry(&app)?;
    set_active_local_profile(&conn, &profile_id)
        .map_err(|error| format!("切换本地账号失败：{error}"))?;
    drop(conn);

    let (paths, conn) = open_library(&app)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
pub(crate) fn reset_library_data(app: AppHandle) -> Result<LibraryOverview, String> {
    let (paths, mut conn) = open_library(&app)?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute_batch(
            "DELETE FROM sync_run_items;
             DELETE FROM sync_runs;
             DELETE FROM sync_checkpoints;
             DELETE FROM media_assets;
             DELETE FROM note_tags;
             DELETE FROM notes;
             DELETE FROM tags;
             DELETE FROM categories;
             DELETE FROM accounts;",
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;

    if paths.media_dir.exists() {
        fs::remove_dir_all(&paths.media_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(paths.media_dir.join("videos")).map_err(|error| error.to_string())?;
    fs::create_dir_all(paths.media_dir.join("images")).map_err(|error| error.to_string())?;
    ensure_storage_root(&conn, &paths)?;

    library_overview(&app, &paths, &conn)
}

#[tauri::command]
pub(crate) fn delete_library_database(app: AppHandle) -> Result<LibraryOverview, String> {
    let paths = resolve_paths(&app)?;
    if paths.db_path.exists() {
        fs::remove_file(&paths.db_path).map_err(|error| error.to_string())?;
    }
    let (paths, conn) = open_library(&app)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
pub(crate) fn clear_media_files(app: AppHandle) -> Result<LibraryOverview, String> {
    let (paths, conn) = open_library(&app)?;
    if paths.media_dir.exists() {
        fs::remove_dir_all(&paths.media_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(paths.media_dir.join("videos")).map_err(|error| error.to_string())?;
    fs::create_dir_all(paths.media_dir.join("images")).map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE media_assets
         SET relative_path = NULL,
             size_bytes = NULL,
             download_status = 'not_downloaded',
             download_error = NULL,
             downloaded_at = NULL,
             updated_at = CURRENT_TIMESTAMP
         WHERE COALESCE(relative_path, '') <> ''
            OR COALESCE(download_status, 'not_downloaded') <> 'not_downloaded'
            OR download_error IS NOT NULL
            OR downloaded_at IS NOT NULL",
        [],
    )
    .map_err(|error| error.to_string())?;
    ensure_storage_root(&conn, &paths)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
pub(crate) fn list_notes(app: AppHandle) -> Result<Vec<NoteSummary>, String> {
    let (_, conn) = open_library(&app)?;
    read_notes(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn update_note_status(
    app: AppHandle,
    input: StatusUpdateInput,
) -> Result<Vec<NoteSummary>, String> {
    validate_note_status(&input.status)?;

    let (_, conn) = open_library(&app)?;
    conn.execute(
        "UPDATE notes SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![input.status, &input.note_id],
    )
    .map_err(|error| error.to_string())?;

    read_notes(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn update_note_metadata(
    app: AppHandle,
    input: NoteMetadataUpdateInput,
) -> Result<Vec<NoteSummary>, String> {
    if let Some(status) = input.status.as_deref() {
        validate_note_status(status)?;
    }

    let (_, mut conn) = open_library(&app)?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;

    if let Some(status) = input.status.as_deref() {
        transaction
            .execute(
                "UPDATE notes SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![status, &input.note_id],
            )
            .map_err(|error| error.to_string())?;
    }

    if let Some(user_note) = input.user_note.as_deref() {
        transaction
            .execute(
                "UPDATE notes SET user_note = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![user_note, &input.note_id],
            )
            .map_err(|error| error.to_string())?;
    }

    if let Some(category_name) = input.category_name.as_deref() {
        let category_id = upsert_optional_category(&transaction, category_name)
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE notes SET category_id = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![category_id.as_deref(), &input.note_id],
            )
            .map_err(|error| error.to_string())?;
    }

    if let Some(tags) = input.tags {
        replace_user_tags_for_note(&transaction, &input.note_id, tags)
            .map_err(|error| error.to_string())?;
    }

    transaction.commit().map_err(|error| error.to_string())?;
    read_notes(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn batch_update_note_metadata(
    app: AppHandle,
    input: BatchNoteMetadataUpdateInput,
) -> Result<Vec<NoteSummary>, String> {
    if let Some(status) = input.status.as_deref() {
        validate_note_status(status)?;
    }

    let note_ids = normalize_id_list(input.note_ids);
    if note_ids.is_empty() {
        return Err("没有选择要更新的收藏。".to_string());
    }

    let add_tags = normalize_tag_names(input.add_tags.unwrap_or_default());
    let (_, mut conn) = open_library(&app)?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let category_id = if let Some(category_name) = input.category_name.as_deref() {
        Some(
            upsert_optional_category(&transaction, category_name)
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };

    for note_id in &note_ids {
        if let Some(status) = input.status.as_deref() {
            transaction
                .execute(
                    "UPDATE notes SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                    params![status, note_id],
                )
                .map_err(|error| error.to_string())?;
        }
        if let Some(next_category_id) = category_id.as_ref() {
            transaction
                .execute(
                    "UPDATE notes SET category_id = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                    params![next_category_id.as_deref(), note_id],
                )
                .map_err(|error| error.to_string())?;
        }
        for tag_name in &add_tags {
            let tag_id = upsert_tag_with_kind(&transaction, tag_name, "user")
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
                    params![note_id, tag_id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    transaction.commit().map_err(|error| error.to_string())?;
    read_notes(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn export_library(
    app: AppHandle,
    input: ExportLibraryInput,
) -> Result<ExportLibraryResult, String> {
    let format = match input.format.trim().to_lowercase().as_str() {
        "json" => "json".to_string(),
        "csv" => "csv".to_string(),
        "markdown" | "md" => "markdown".to_string(),
        _ => return Err("Unsupported export format".to_string()),
    };
    let include_media = input.include_media.unwrap_or(true);
    let include_notes = input.include_notes.unwrap_or(true);
    let only_reviewed = input.only_reviewed.unwrap_or(false);

    let (paths, conn) = open_library(&app)?;
    let mut notes = read_notes(&conn).map_err(|error| error.to_string())?;
    if only_reviewed {
        notes.retain(|note| note.status != "unread");
    }
    let media_count = if include_media {
        notes.iter().map(|note| note.media.len()).sum()
    } else {
        0
    };

    let body = match format.as_str() {
        "json" => export_notes_json(&notes, include_media, include_notes)?,
        "csv" => export_notes_csv(&notes, include_media, include_notes),
        "markdown" => export_notes_markdown(&notes, include_media, include_notes),
        _ => unreachable!(),
    };

    let export_dir = paths.app_data_dir.join("exports");
    fs::create_dir_all(&export_dir).map_err(|error| {
        format!(
            "创建导出目录失败 {}：{error}",
            display_path(export_dir.clone())
        )
    })?;
    let extension = if format == "markdown" {
        "md"
    } else {
        format.as_str()
    };
    let file_name = format!(
        "xhs-collection-{}.{}",
        Utc::now().format("%Y%m%d-%H%M%S"),
        extension
    );
    let export_path = export_dir.join(file_name);
    fs::write(&export_path, body).map_err(|error| {
        format!(
            "写入导出文件失败 {}：{error}",
            display_path(export_path.clone())
        )
    })?;

    Ok(ExportLibraryResult {
        path: display_path(export_path.clone()),
        format,
        note_count: notes.len(),
        media_count,
        message: format!(
            "已导出 {} 条收藏{}到 {}。",
            notes.len(),
            if include_media {
                format!("、{} 个媒体路径", media_count)
            } else {
                String::new()
            },
            display_path(export_path)
        ),
    })
}

fn validate_note_status(status: &str) -> Result<(), String> {
    if matches!(status, "unread" | "read" | "outdated" | "archived") {
        Ok(())
    } else {
        Err("Unsupported note status".to_string())
    }
}

fn export_notes_json(
    notes: &[NoteSummary],
    include_media: bool,
    include_notes: bool,
) -> Result<String, String> {
    let exported_notes = notes
        .iter()
        .map(|note| {
            let media = if include_media {
                note.media
                    .iter()
                    .map(|asset| {
                        json!({
                            "id": &asset.id,
                            "type": &asset.media_type,
                            "downloadStatus": &asset.download_status,
                            "originalUrl": &asset.original_url,
                            "relativePath": &asset.relative_path,
                            "mimeType": &asset.mime_type,
                            "sizeBytes": asset.size_bytes,
                            "width": asset.width,
                            "height": asset.height,
                            "durationMs": asset.duration_ms
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let review = if include_notes {
                json!({
                    "status": &note.status,
                    "categoryName": &note.category_name,
                    "userNote": &note.user_note,
                    "tags": &note.tags
                })
            } else {
                json!({})
            };
            json!({
                "id": &note.id,
                "source": &note.source,
                "sourceNoteId": &note.source_note_id,
                "sourceUrl": &note.source_url,
                "title": &note.title,
                "excerpt": &note.excerpt,
                "content": &note.content,
                "authorName": &note.author_name,
                "coverUrl": &note.cover_url,
                "noteType": &note.note_type,
                "publishedAt": &note.published_at,
                "collectedAt": &note.collected_at,
                "favoriteOrder": note.favorite_order,
                "remoteStatus": &note.remote_status,
                "review": review,
                "media": media
            })
        })
        .collect::<Vec<_>>();

    let payload = json!({
        "exportedAt": Utc::now().to_rfc3339(),
        "app": "XHS Collection",
        "noteCount": notes.len(),
        "notes": exported_notes
    });
    serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())
}

fn export_notes_csv(notes: &[NoteSummary], include_media: bool, include_notes: bool) -> String {
    let mut rows = Vec::new();
    let mut header = vec![
        "id",
        "source_note_id",
        "title",
        "author",
        "type",
        "collected_at",
        "published_at",
        "source_url",
        "remote_status",
    ];
    if include_notes {
        header.extend(["status", "category", "tags", "user_note"]);
    }
    if include_media {
        header.extend(["media_count", "media_paths"]);
    }
    rows.push(header.join(","));

    for note in notes {
        let mut row = vec![
            csv_escape(&note.id),
            csv_escape(&note.source_note_id),
            csv_escape(&note.title),
            csv_escape(&note.author_name),
            csv_escape(&note.note_type),
            csv_escape(note.collected_at.as_deref().unwrap_or_default()),
            csv_escape(note.published_at.as_deref().unwrap_or_default()),
            csv_escape(&note.source_url),
            csv_escape(&note.remote_status),
        ];
        if include_notes {
            row.extend([
                csv_escape(&note.status),
                csv_escape(note.category_name.as_deref().unwrap_or_default()),
                csv_escape(&note.tags.join("; ")),
                csv_escape(&note.user_note),
            ]);
        }
        if include_media {
            let paths = note
                .media
                .iter()
                .filter_map(|asset| asset.relative_path.as_deref())
                .collect::<Vec<_>>()
                .join("; ");
            row.extend([note.media.len().to_string(), csv_escape(&paths)]);
        }
        rows.push(row.join(","));
    }

    format!("{}\n", rows.join("\n"))
}

fn export_notes_markdown(
    notes: &[NoteSummary],
    include_media: bool,
    include_notes: bool,
) -> String {
    let mut output = String::new();
    output.push_str("# XHS Collection Export\n\n");
    output.push_str(&format!(
        "- Exported at: {}\n- Notes: {}\n\n",
        Utc::now().to_rfc3339(),
        notes.len()
    ));

    for (index, note) in notes.iter().enumerate() {
        output.push_str(&format!(
            "## {}. {}\n\n",
            index + 1,
            markdown_inline(&note.title)
        ));
        output.push_str(&format!(
            "- Author: {}\n- Type: {}\n- Collected: {}\n- Source: {}\n",
            markdown_inline(&note.author_name),
            note.note_type,
            note.collected_at.as_deref().unwrap_or(""),
            note.source_url
        ));
        if include_notes {
            output.push_str(&format!(
                "- Status: {}\n- Category: {}\n- Tags: {}\n",
                note.status,
                markdown_inline(note.category_name.as_deref().unwrap_or("")),
                markdown_inline(&note.tags.join(", "))
            ));
            if !note.user_note.trim().is_empty() {
                output.push_str(&format!("\n> {}\n", note.user_note.replace('\n', "\n> ")));
            }
        }
        let body = if note.content.trim().is_empty() {
            note.excerpt.trim()
        } else {
            note.content.trim()
        };
        if !body.is_empty() {
            output.push_str(&format!("\n{}\n", body));
        }
        if include_media && !note.media.is_empty() {
            output.push_str("\nMedia:\n");
            for asset in &note.media {
                output.push_str(&format!(
                    "- {} · {} · {}\n",
                    asset.media_type,
                    asset.download_status,
                    asset
                        .relative_path
                        .as_deref()
                        .or(asset.original_url.as_deref())
                        .unwrap_or("")
                ));
            }
        }
        output.push('\n');
    }
    output
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn markdown_inline(value: &str) -> String {
    value.replace('\n', " ").trim().to_string()
}
