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
use std::io::{Read, Seek, Write};
use std::path::Path;
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
             DELETE FROM album_notes;
             DELETE FROM albums;
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

#[tauri::command]
pub(crate) fn create_library_backup(app: AppHandle) -> Result<LibraryBackupResult, String> {
    let (paths, conn) = open_library(&app)?;
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(FULL);");
    let overview = library_overview(&app, &paths, &conn)?;
    drop(conn);

    let backup_dir = paths.app_data_dir.join("backups");
    fs::create_dir_all(&backup_dir).map_err(|error| {
        format!(
            "创建备份目录失败 {}：{error}",
            display_path(backup_dir.clone())
        )
    })?;
    let backup_path = backup_dir.join(format!(
        "xhs-collection-backup-{}.zip",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));

    let backup_file = fs::File::create(&backup_path).map_err(|error| {
        format!(
            "创建备份文件失败 {}：{error}",
            display_path(backup_path.clone())
        )
    })?;
    let mut zip = StoredZipWriter::new(backup_file);
    let manifest = json!({
        "app": "XHS Collection",
        "createdAt": Utc::now().to_rfc3339(),
        "profile": {
            "id": overview.active_profile.id,
            "displayName": overview.active_profile.display_name,
            "source": overview.active_profile.source,
            "sourceAccountId": overview.active_profile.source_account_id,
        },
        "counts": {
            "notes": overview.notes_count,
            "mediaAssets": overview.media_count,
        },
        "paths": {
            "database": "library.sqlite",
            "media": "media/",
        }
    });
    zip.add_bytes("manifest.json", manifest.to_string().as_bytes())
        .map_err(|error| format!("写入备份清单失败：{error}"))?;

    if paths.db_path.exists() {
        zip.add_file(&paths.db_path, "library.sqlite")
            .map_err(|error| format!("写入 SQLite 备份失败：{error}"))?;
    }

    let profile_registry = paths
        .app_data_dir
        .join(crate::constants::PROFILE_REGISTRY_DB);
    if profile_registry.exists() {
        zip.add_file(&profile_registry, crate::constants::PROFILE_REGISTRY_DB)
            .map_err(|error| format!("写入本地账号索引失败：{error}"))?;
    }

    if paths.media_dir.exists() {
        add_directory_to_backup(&mut zip, &paths.media_dir, "media")
            .map_err(|error| format!("写入媒体目录备份失败：{error}"))?;
    }

    let file_count = zip.file_count();
    zip.finish()
        .map_err(|error| format!("完成备份文件失败：{error}"))?;
    let size_bytes = fs::metadata(&backup_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or(0);

    Ok(LibraryBackupResult {
        path: display_path(backup_path.clone()),
        file_count,
        size_bytes,
        message: format!(
            "已生成整库备份：{} 个文件，{}。",
            file_count,
            display_path(backup_path)
        ),
    })
}

fn add_directory_to_backup<W: Write + Seek>(
    zip: &mut StoredZipWriter<W>,
    root: &Path,
    zip_root: &str,
) -> Result<(), String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|error| format!("读取目录失败 {}：{error}", display_path(dir.clone())))?;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            let zip_name = format!("{}/{}", zip_root.trim_end_matches('/'), zip_path(relative));
            zip.add_file(&path, &zip_name)?;
        }
    }
    Ok(())
}

fn zip_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(sanitize_zip_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn sanitize_zip_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

struct StoredZipWriter<W: Write + Seek> {
    inner: W,
    entries: Vec<ZipEntry>,
}

struct ZipEntry {
    name: String,
    crc32: u32,
    size: u32,
    local_header_offset: u32,
}

impl<W: Write + Seek> StoredZipWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            entries: Vec::new(),
        }
    }

    fn file_count(&self) -> usize {
        self.entries.len()
    }

    fn add_file(&mut self, path: &Path, zip_name: &str) -> Result<(), String> {
        let mut file = fs::File::open(path).map_err(|error| {
            format!("打开文件失败 {}：{error}", display_path(path.to_path_buf()))
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|error| {
            format!("读取文件失败 {}：{error}", display_path(path.to_path_buf()))
        })?;
        self.add_bytes(zip_name, &bytes)
    }

    fn add_bytes(&mut self, zip_name: &str, bytes: &[u8]) -> Result<(), String> {
        let name = normalize_zip_name(zip_name)?;
        let size = u32::try_from(bytes.len())
            .map_err(|_| format!("备份文件过大，无法写入 zip：{name}"))?;
        let offset = u32::try_from(
            self.inner
                .stream_position()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|_| "备份文件超过 zip32 大小限制。".to_string())?;
        let crc32 = crc32(bytes);
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| format!("备份路径过长，无法写入 zip：{name}"))?;

        write_u32(&mut self.inner, 0x0403_4b50)?;
        write_u16(&mut self.inner, 20)?;
        write_u16(&mut self.inner, 0)?;
        write_u16(&mut self.inner, 0)?;
        write_u16(&mut self.inner, 0)?;
        write_u16(&mut self.inner, 0)?;
        write_u32(&mut self.inner, crc32)?;
        write_u32(&mut self.inner, size)?;
        write_u32(&mut self.inner, size)?;
        write_u16(&mut self.inner, name_len)?;
        write_u16(&mut self.inner, 0)?;
        self.inner
            .write_all(name_bytes)
            .map_err(|error| error.to_string())?;
        self.inner
            .write_all(bytes)
            .map_err(|error| error.to_string())?;

        self.entries.push(ZipEntry {
            name,
            crc32,
            size,
            local_header_offset: offset,
        });
        Ok(())
    }

    fn finish(mut self) -> Result<(), String> {
        let central_dir_offset = u32::try_from(
            self.inner
                .stream_position()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|_| "备份文件超过 zip32 大小限制。".to_string())?;

        for entry in &self.entries {
            let name_bytes = entry.name.as_bytes();
            let name_len = u16::try_from(name_bytes.len())
                .map_err(|_| format!("备份路径过长，无法写入 zip：{}", entry.name))?;
            write_u32(&mut self.inner, 0x0201_4b50)?;
            write_u16(&mut self.inner, 20)?;
            write_u16(&mut self.inner, 20)?;
            write_u16(&mut self.inner, 0)?;
            write_u16(&mut self.inner, 0)?;
            write_u16(&mut self.inner, 0)?;
            write_u16(&mut self.inner, 0)?;
            write_u32(&mut self.inner, entry.crc32)?;
            write_u32(&mut self.inner, entry.size)?;
            write_u32(&mut self.inner, entry.size)?;
            write_u16(&mut self.inner, name_len)?;
            write_u16(&mut self.inner, 0)?;
            write_u16(&mut self.inner, 0)?;
            write_u16(&mut self.inner, 0)?;
            write_u16(&mut self.inner, 0)?;
            write_u32(&mut self.inner, 0)?;
            write_u32(&mut self.inner, entry.local_header_offset)?;
            self.inner
                .write_all(name_bytes)
                .map_err(|error| error.to_string())?;
        }

        let central_dir_size = u32::try_from(
            self.inner
                .stream_position()
                .map_err(|error| error.to_string())?
                - u64::from(central_dir_offset),
        )
        .map_err(|_| "备份文件超过 zip32 大小限制。".to_string())?;
        let entry_count = u16::try_from(self.entries.len())
            .map_err(|_| "备份文件数量超过 zip32 限制。".to_string())?;
        write_u32(&mut self.inner, 0x0605_4b50)?;
        write_u16(&mut self.inner, 0)?;
        write_u16(&mut self.inner, 0)?;
        write_u16(&mut self.inner, entry_count)?;
        write_u16(&mut self.inner, entry_count)?;
        write_u32(&mut self.inner, central_dir_size)?;
        write_u32(&mut self.inner, central_dir_offset)?;
        write_u16(&mut self.inner, 0)?;
        self.inner.flush().map_err(|error| error.to_string())
    }
}

fn normalize_zip_name(name: &str) -> Result<String, String> {
    let normalized = name.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains("../")
        || normalized.contains("/..")
    {
        return Err(format!("非法备份路径：{name}"));
    }
    Ok(normalized)
}

fn write_u16<W: Write>(writer: &mut W, value: u16) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| error.to_string())
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| error.to_string())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
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
