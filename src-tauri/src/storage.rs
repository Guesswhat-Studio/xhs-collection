use crate::constants::*;
use crate::models::*;
use crate::utils::display_path;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
pub(crate) fn open_library(app: &AppHandle) -> Result<(LibraryPaths, Connection), String> {
    let paths = resolve_paths(app)?;
    fs::create_dir_all(paths.media_dir.join("videos")).map_err(|error| error.to_string())?;
    fs::create_dir_all(paths.media_dir.join("images")).map_err(|error| error.to_string())?;

    let conn = Connection::open(&paths.db_path).map_err(|error| error.to_string())?;
    if let Err(first_error) = conn.execute_batch(INITIAL_SCHEMA) {
        ensure_schema_upgrades(&conn)
            .map_err(|error| format!("数据库升级失败：{error}；初始错误：{first_error}"))?;
        conn.execute_batch(INITIAL_SCHEMA)
            .map_err(|error| format!("数据库初始化失败：{error}；初始错误：{first_error}"))?;
    }
    ensure_schema_upgrades(&conn).map_err(|error| error.to_string())?;
    ensure_storage_root(&conn, &paths)?;

    Ok((paths, conn))
}

pub(crate) fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    Ok(app_data_dir)
}

pub(crate) fn library_overview(
    app: &AppHandle,
    paths: &LibraryPaths,
    conn: &Connection,
) -> Result<LibraryOverview, String> {
    let notes_count = count_rows(conn, "notes")?;
    let media_count = count_rows(conn, "media_assets")?;
    let content_coverage = library_content_coverage(conn).map_err(|error| error.to_string())?;
    let (_, registry) = open_profile_registry(app)?;
    let profiles = read_local_profiles(&registry).map_err(|error| error.to_string())?;
    let summaries = profile_summaries(&paths.app_data_dir, profiles);
    let active_profile = summaries
        .iter()
        .find(|profile| profile.id == paths.profile.id)
        .cloned()
        .unwrap_or_else(|| local_profile_summary(&paths.app_data_dir, &paths.profile));

    Ok(LibraryOverview {
        app_data_dir: display_path(paths.app_data_dir.clone()),
        db_path: display_path(paths.db_path.clone()),
        media_dir: display_path(paths.media_dir.clone()),
        notes_count,
        media_count,
        content_coverage,
        storage_root_id: STORAGE_ROOT_ID.to_string(),
        active_profile,
        profiles: summaries,
    })
}

fn library_content_coverage(conn: &Connection) -> rusqlite::Result<LibraryContentCoverage> {
    let total_notes = count_rows_sql(conn, "SELECT COUNT(*) FROM notes")?;
    let detail_notes = count_rows_sql(
        conn,
        "SELECT COUNT(*) FROM notes WHERE COALESCE(content, '') <> ''",
    )?;
    let tagged_notes = count_rows_sql(conn, "SELECT COUNT(DISTINCT note_id) FROM note_tags")?;
    let media_notes = count_rows_sql(conn, "SELECT COUNT(DISTINCT note_id) FROM media_assets")?;
    let unique_tags = count_rows_sql(conn, "SELECT COUNT(*) FROM tags")?;

    Ok(LibraryContentCoverage {
        total_notes,
        detail_notes,
        tagged_notes,
        media_notes,
        missing_detail_notes: total_notes.saturating_sub(detail_notes),
        missing_tag_notes: total_notes.saturating_sub(tagged_notes),
        unique_tags,
    })
}

fn count_rows_sql(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
}

pub(crate) fn resolve_paths(app: &AppHandle) -> Result<LibraryPaths, String> {
    let app_data_dir = app_data_dir(app)?;
    let (_, registry) = open_profile_registry(app)?;
    let profile = read_active_local_profile(&registry)
        .map_err(|error| format!("读取当前本地账号失败：{error}"))?;

    let db_path = app_data_dir.join(Path::new(&profile.db_relative_path));
    let media_dir = app_data_dir.join(Path::new(&profile.media_relative_path));
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&media_dir).map_err(|error| error.to_string())?;

    Ok(LibraryPaths {
        app_data_dir,
        db_path,
        media_dir,
        profile,
    })
}

pub(crate) fn open_profile_registry(app: &AppHandle) -> Result<(PathBuf, Connection), String> {
    let app_data_dir = app_data_dir(app)?;
    let db_path = app_data_dir.join(PROFILE_REGISTRY_DB);
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    ensure_profile_registry(&conn).map_err(|error| error.to_string())?;
    Ok((db_path, conn))
}

pub(crate) fn ensure_profile_registry(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS local_profiles (
           id TEXT PRIMARY KEY,
           display_name TEXT,
           source TEXT,
           source_account_id TEXT,
           avatar_url TEXT,
           db_relative_path TEXT NOT NULL,
           media_relative_path TEXT NOT NULL,
           is_active INTEGER NOT NULL DEFAULT 0,
           session_status TEXT NOT NULL DEFAULT 'unknown',
           last_opened_at TEXT,
           last_sync_at TEXT,
           created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
           UNIQUE(source, source_account_id)
         );
         CREATE INDEX IF NOT EXISTS idx_local_profiles_active ON local_profiles(is_active);
         CREATE INDEX IF NOT EXISTS idx_local_profiles_source ON local_profiles(source, source_account_id);",
    )?;

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM local_profiles", [], |row| row.get(0))?;
    if count == 0 {
        let (db_relative_path, media_relative_path) =
            ("library.sqlite".to_string(), "media".to_string());
        conn.execute(
            "INSERT OR IGNORE INTO local_profiles (
                id, display_name, db_relative_path, media_relative_path,
                is_active, session_status, last_opened_at
             )
             VALUES (?1, '本机账号', ?2, ?3, 1, 'unknown', CURRENT_TIMESTAMP)",
            params![DEFAULT_PROFILE_ID, db_relative_path, media_relative_path],
        )?;
    }

    let active_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM local_profiles WHERE is_active = 1",
        [],
        |row| row.get(0),
    )?;
    if active_count == 0 {
        conn.execute(
            "UPDATE local_profiles
             SET is_active = 1,
                 last_opened_at = CURRENT_TIMESTAMP,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = (
                SELECT id FROM local_profiles
                ORDER BY datetime(COALESCE(last_opened_at, updated_at, created_at)) DESC
                LIMIT 1
             )",
            [],
        )?;
    } else if active_count > 1 {
        let active_id: String = conn.query_row(
            "SELECT id FROM local_profiles
             WHERE is_active = 1
             ORDER BY datetime(COALESCE(last_opened_at, updated_at, created_at)) DESC
             LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "UPDATE local_profiles SET is_active = CASE WHEN id = ?1 THEN 1 ELSE 0 END",
            params![active_id],
        )?;
    }

    Ok(())
}

pub(crate) fn read_local_profiles(conn: &Connection) -> rusqlite::Result<Vec<LocalProfile>> {
    let mut stmt = conn.prepare(
        "SELECT
            id, display_name, source, source_account_id, avatar_url,
            db_relative_path, media_relative_path, is_active, session_status,
            last_opened_at, last_sync_at, created_at, updated_at
         FROM local_profiles
         ORDER BY is_active DESC,
            datetime(COALESCE(last_opened_at, updated_at, created_at)) DESC,
            datetime(created_at) ASC",
    )?;
    let rows = stmt.query_map([], local_profile_from_row)?;
    rows.collect()
}

pub(crate) fn read_active_local_profile(conn: &Connection) -> rusqlite::Result<LocalProfile> {
    conn.query_row(
        "SELECT
            id, display_name, source, source_account_id, avatar_url,
            db_relative_path, media_relative_path, is_active, session_status,
            last_opened_at, last_sync_at, created_at, updated_at
         FROM local_profiles
         WHERE is_active = 1
         ORDER BY datetime(COALESCE(last_opened_at, updated_at, created_at)) DESC
         LIMIT 1",
        [],
        local_profile_from_row,
    )
}

pub(crate) fn find_local_profile_by_xhs_id(
    conn: &Connection,
    user_id: &str,
) -> rusqlite::Result<Option<LocalProfile>> {
    conn.query_row(
        "SELECT
            id, display_name, source, source_account_id, avatar_url,
            db_relative_path, media_relative_path, is_active, session_status,
            last_opened_at, last_sync_at, created_at, updated_at
         FROM local_profiles
         WHERE source = 'xhs' AND source_account_id = ?1
         LIMIT 1",
        params![user_id],
        local_profile_from_row,
    )
    .optional()
}

pub(crate) fn local_profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalProfile> {
    Ok(LocalProfile {
        id: row.get(0)?,
        display_name: row.get(1)?,
        source: row.get(2)?,
        source_account_id: row.get(3)?,
        avatar_url: row.get(4)?,
        db_relative_path: row.get(5)?,
        media_relative_path: row.get(6)?,
        is_active: row.get::<_, i64>(7)? != 0,
        session_status: row.get(8)?,
        last_opened_at: row.get(9)?,
        last_sync_at: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

pub(crate) fn profile_summaries(
    app_data_dir: &Path,
    profiles: Vec<LocalProfile>,
) -> Vec<LocalProfileSummary> {
    profiles
        .iter()
        .map(|profile| local_profile_summary(app_data_dir, profile))
        .collect()
}

pub(crate) fn local_profile_summary(
    app_data_dir: &Path,
    profile: &LocalProfile,
) -> LocalProfileSummary {
    LocalProfileSummary {
        id: profile.id.clone(),
        display_name: profile
            .display_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "本机账号".to_string()),
        source: profile.source.clone(),
        source_account_id: profile.source_account_id.clone(),
        avatar_url: profile.avatar_url.clone(),
        db_path: display_path(app_data_dir.join(Path::new(&profile.db_relative_path))),
        media_dir: display_path(app_data_dir.join(Path::new(&profile.media_relative_path))),
        is_active: profile.is_active,
        session_status: profile.session_status.clone(),
        last_opened_at: profile.last_opened_at.clone(),
        last_sync_at: profile.last_sync_at.clone(),
        created_at: profile.created_at.clone(),
        updated_at: profile.updated_at.clone(),
    }
}

pub(crate) fn set_active_local_profile(
    conn: &Connection,
    profile_id: &str,
) -> rusqlite::Result<()> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM local_profiles WHERE id = ?1",
            params![profile_id],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    conn.execute(
        "UPDATE local_profiles
         SET is_active = CASE WHEN id = ?1 THEN 1 ELSE 0 END,
             last_opened_at = CASE WHEN id = ?1 THEN CURRENT_TIMESTAMP ELSE last_opened_at END,
             updated_at = CASE WHEN id = ?1 THEN CURRENT_TIMESTAMP ELSE updated_at END",
        params![profile_id],
    )?;
    Ok(())
}

pub(crate) fn ensure_storage_root(conn: &Connection, paths: &LibraryPaths) -> Result<(), String> {
    conn.execute(
        "INSERT INTO storage_roots (id, kind, platform, base_dir, relative_path, absolute_path)
         VALUES (?1, 'app_local_data', 'all', '$APPLOCALDATA', ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
            relative_path = excluded.relative_path,
            absolute_path = excluded.absolute_path,
            updated_at = CURRENT_TIMESTAMP",
        params![
            STORAGE_ROOT_ID,
            paths.profile.media_relative_path.as_str(),
            display_path(paths.media_dir.clone())
        ],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub(crate) fn ensure_schema_upgrades(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_settings (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL DEFAULT 'openai_compatible',
            base_url TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            api_key_key_id TEXT,
            api_key_storage TEXT NOT NULL DEFAULT 'none',
            api_key_fallback TEXT NOT NULL DEFAULT '',
            temperature REAL NOT NULL DEFAULT 0.2,
            max_tokens INTEGER NOT NULL DEFAULT 4096,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );",
    )?;
    add_column_if_missing(conn, "accounts", "session_cookie", "session_cookie TEXT")?;
    add_column_if_missing(conn, "accounts", "session_key_id", "session_key_id TEXT")?;
    add_column_if_missing(
        conn,
        "accounts",
        "session_storage",
        "session_storage TEXT NOT NULL DEFAULT 'sqlite'",
    )?;
    add_column_if_missing(
        conn,
        "accounts",
        "session_checked_at",
        "session_checked_at TEXT",
    )?;
    add_column_if_missing(conn, "notes", "collected_at", "collected_at TEXT")?;
    add_column_if_missing(conn, "notes", "favorite_order", "favorite_order INTEGER")?;
    add_column_if_missing(conn, "notes", "last_seen_at", "last_seen_at TEXT")?;
    add_column_if_missing(
        conn,
        "notes",
        "remote_status",
        "remote_status TEXT NOT NULL DEFAULT 'available'",
    )?;
    add_column_if_missing(
        conn,
        "notes",
        "unavailable_reason",
        "unavailable_reason TEXT",
    )?;
    add_column_if_missing(conn, "notes", "author_id", "author_id TEXT")?;
    add_column_if_missing(conn, "notes", "remote_updated_at", "remote_updated_at TEXT")?;
    add_column_if_missing(
        conn,
        "albums",
        "source",
        "source TEXT NOT NULL DEFAULT 'local'",
    )?;
    add_column_if_missing(conn, "albums", "source_album_id", "source_album_id TEXT")?;
    add_column_if_missing(
        conn,
        "albums",
        "source_account_id",
        "source_account_id TEXT",
    )?;
    add_column_if_missing(conn, "albums", "source_url", "source_url TEXT")?;
    add_column_if_missing(conn, "albums", "cover_url", "cover_url TEXT")?;
    add_column_if_missing(conn, "albums", "note_count", "note_count INTEGER")?;
    add_column_if_missing(conn, "albums", "raw_json", "raw_json TEXT")?;
    add_column_if_missing(conn, "albums", "last_synced_at", "last_synced_at TEXT")?;
    add_column_if_missing(conn, "tags", "ai_group", "ai_group TEXT")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tag_aliases (
            id TEXT PRIMARY KEY,
            alias_name TEXT NOT NULL,
            normalized_alias TEXT NOT NULL UNIQUE,
            canonical_tag_id TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'manual',
            confidence REAL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (canonical_tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );",
    )?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "scanned_count",
        "scanned_count INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "existing_skipped_count",
        "existing_skipped_count INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "remote_missing_count",
        "remote_missing_count INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "reached_end",
        "reached_end INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "limit_reached",
        "limit_reached INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(conn, "sync_runs", "stop_reason", "stop_reason TEXT")?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "first_source_note_id",
        "first_source_note_id TEXT",
    )?;
    add_column_if_missing(
        conn,
        "sync_runs",
        "last_source_note_id",
        "last_source_note_id TEXT",
    )?;
    add_column_if_missing(
        conn,
        "sync_checkpoints",
        "remote_display_count",
        "remote_display_count INTEGER",
    )?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_albums_source
            ON albums(source, source_account_id, source_album_id);
         CREATE INDEX IF NOT EXISTS idx_album_notes_note_id
            ON album_notes(note_id);
         CREATE INDEX IF NOT EXISTS idx_tag_aliases_canonical
            ON tag_aliases(canonical_tag_id);",
    )?;
    Ok(())
}

pub(crate) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    column_sql: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(());
        }
    }
    conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column_sql}"), [])?;
    Ok(())
}

pub(crate) fn count_rows(conn: &Connection, table: &str) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

pub(crate) fn read_notes(conn: &Connection) -> rusqlite::Result<Vec<NoteSummary>> {
    let mut stmt = conn.prepare(
        "SELECT
            n.id,
            n.source,
            n.source_note_id,
            n.source_url,
            COALESCE(n.title, ''),
            COALESCE(n.excerpt, ''),
            COALESCE(n.content, ''),
            COALESCE(n.author_name, ''),
            n.cover_url,
            n.note_type,
            n.published_at,
            n.collected_at,
            n.favorite_order,
            n.last_synced_at,
            n.last_seen_at,
            n.remote_missing_at,
            n.remote_status,
            n.unavailable_reason,
            n.status,
            c.name,
            n.user_note
         FROM notes n
         LEFT JOIN categories c ON c.id = n.category_id
         ORDER BY
            CASE WHEN n.collected_at IS NULL OR n.collected_at = '' THEN 1 ELSE 0 END ASC,
            datetime(n.collected_at) DESC,
            COALESCE(n.favorite_order, 999999999) ASC,
            datetime(n.last_seen_at) DESC,
            datetime(n.last_synced_at) DESC,
            datetime(n.updated_at) DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(BaseNote {
            id: row.get(0)?,
            source: row.get(1)?,
            source_note_id: row.get(2)?,
            source_url: row.get(3)?,
            title: row.get(4)?,
            excerpt: row.get(5)?,
            content: row.get(6)?,
            author_name: row.get(7)?,
            cover_url: row.get(8)?,
            note_type: row.get(9)?,
            published_at: row.get(10)?,
            collected_at: row.get(11)?,
            favorite_order: row.get(12)?,
            last_synced_at: row.get(13)?,
            last_seen_at: row.get(14)?,
            remote_missing_at: row.get(15)?,
            remote_status: row.get(16)?,
            unavailable_reason: row.get(17)?,
            status: row.get(18)?,
            category_name: row.get(19)?,
            user_note: row.get(20)?,
        })
    })?;

    let mut notes = Vec::new();
    for row in rows {
        let base = row?;
        let tags = read_tags(conn, &base.id)?;
        let media = read_media(conn, &base.id)?;
        notes.push(NoteSummary {
            id: base.id,
            source: base.source,
            source_note_id: base.source_note_id,
            source_url: base.source_url,
            title: base.title,
            excerpt: base.excerpt,
            content: base.content,
            author_name: base.author_name,
            cover_url: base.cover_url,
            note_type: base.note_type,
            published_at: base.published_at,
            collected_at: base.collected_at,
            favorite_order: base.favorite_order,
            last_synced_at: base.last_synced_at,
            last_seen_at: base.last_seen_at,
            remote_missing_at: base.remote_missing_at,
            remote_status: base.remote_status,
            unavailable_reason: base.unavailable_reason,
            status: base.status,
            category_name: base.category_name,
            user_note: base.user_note,
            tags,
            media,
        });
    }

    Ok(notes)
}

pub(crate) fn read_tags(conn: &Connection, note_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name
         FROM tags t
         INNER JOIN note_tags nt ON nt.tag_id = t.id
         WHERE nt.note_id = ?1
         ORDER BY t.name",
    )?;
    let rows = stmt.query_map(params![note_id], |row| row.get(0))?;
    rows.collect()
}

pub(crate) fn read_media(conn: &Connection, note_id: &str) -> rusqlite::Result<Vec<MediaAsset>> {
    let mut stmt = conn.prepare(
        "SELECT id, note_id, media_type, download_status, original_url, relative_path, mime_type, size_bytes, width, height, duration_ms
         FROM media_assets
         WHERE note_id = ?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map(params![note_id], |row| {
        Ok(MediaAsset {
            id: row.get(0)?,
            note_id: row.get(1)?,
            media_type: row.get(2)?,
            download_status: row.get(3)?,
            original_url: row.get(4)?,
            relative_path: row.get(5)?,
            mime_type: row.get(6)?,
            size_bytes: row.get(7)?,
            width: row.get(8)?,
            height: row.get(9)?,
            duration_ms: row.get(10)?,
        })
    })?;
    rows.collect()
}

pub(crate) fn upsert_tag_with_kind(
    conn: &Connection,
    name: &str,
    kind: &str,
) -> rusqlite::Result<String> {
    let lookup_key = normalize_tag_alias_key(name);
    if !lookup_key.is_empty() {
        if let Some(canonical_id) = conn
            .query_row(
                "SELECT canonical_tag_id FROM tag_aliases WHERE normalized_alias = ?1 LIMIT 1",
                params![lookup_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            conn.execute(
                "UPDATE tags
                 SET kind = CASE WHEN tags.kind = 'user' THEN tags.kind ELSE ?2 END,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?1",
                params![canonical_id, kind],
            )?;
            return Ok(canonical_id);
        }
    }

    let id = format!("tag:{name}");
    conn.execute(
        "INSERT INTO tags (id, name, kind)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET
            kind = CASE WHEN tags.kind = 'user' THEN tags.kind ELSE excluded.kind END,
            updated_at = CURRENT_TIMESTAMP",
        params![id, name, kind],
    )?;
    conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )
}

pub(crate) fn upsert_optional_category(
    conn: &Connection,
    name: &str,
) -> rusqlite::Result<Option<String>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    if let Some(existing_id) = conn
        .query_row(
            "SELECT id FROM categories WHERE name = ?1 AND parent_id IS NULL LIMIT 1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        conn.execute(
            "UPDATE categories SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![existing_id],
        )?;
        return Ok(Some(existing_id));
    }

    let id = format!("category:{}", Uuid::new_v4());
    conn.execute(
        "INSERT INTO categories (id, name) VALUES (?1, ?2)",
        params![id, name],
    )?;
    Ok(Some(id))
}

pub(crate) fn replace_user_tags_for_note(
    conn: &Connection,
    note_id: &str,
    tags: Vec<String>,
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM note_tags
         WHERE note_id = ?1
           AND tag_id IN (SELECT id FROM tags WHERE kind = 'user')",
        params![note_id],
    )?;

    for tag_name in normalize_tag_names(tags) {
        let tag_id = upsert_tag_with_kind(conn, &tag_name, "user")?;
        conn.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_id, tag_id],
        )?;
    }
    Ok(())
}

pub(crate) fn normalize_tag_names(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for raw in values {
        for part in raw.split([',', '，', ';', '；', '\n', '\t']) {
            let tag = part.trim().trim_start_matches('#').trim();
            if tag.is_empty() || tag.len() > 64 || is_noise_tag_name(tag) {
                continue;
            }
            if seen.insert(tag.to_lowercase()) {
                tags.push(tag.to_string());
            }
            if tags.len() >= 32 {
                return tags;
            }
        }
    }
    tags
}

pub(crate) fn normalize_tag_alias_key(name: &str) -> String {
    name.trim()
        .trim_start_matches(&['#', '＃'][..])
        .chars()
        .filter_map(|ch| {
            let mapped = match ch {
                'Ａ'..='Ｚ' => char::from_u32(ch as u32 - 'Ａ' as u32 + 'A' as u32),
                'ａ'..='ｚ' => char::from_u32(ch as u32 - 'ａ' as u32 + 'a' as u32),
                '０'..='９' => char::from_u32(ch as u32 - '０' as u32 + '0' as u32),
                _ => Some(ch),
            }?;
            if mapped.is_whitespace()
                || matches!(
                    mapped,
                    '_' | '-'
                        | '－'
                        | '—'
                        | '/'
                        | '\\'
                        | '·'
                        | '.'
                        | '。'
                        | ','
                        | '，'
                        | ':'
                        | '：'
                        | ';'
                        | '；'
                        | ' '
                )
            {
                None
            } else {
                Some(mapped.to_lowercase().collect::<String>())
            }
        })
        .collect::<String>()
}

pub(crate) fn is_noise_tag_name(name: &str) -> bool {
    let value = name.trim().trim_start_matches(&['#', '＃'][..]).trim();
    value.is_empty() || is_timecode_tag(value)
}

fn is_timecode_tag(value: &str) -> bool {
    let parts = value.split(':').collect::<Vec<_>>();
    if !(2..=3).contains(&parts.len()) {
        return false;
    }
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return false;
        }
        if index == 0 {
            if part.len() > 2 {
                return false;
            }
        } else if part.len() != 2 {
            return false;
        }
    }
    true
}

pub(crate) fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}
