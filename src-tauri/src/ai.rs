use crate::constants::*;
use crate::models::*;
use crate::secrets::ensure_keyring_store;
use crate::storage::{open_library, read_tags, upsert_optional_category};
use keyring_core::{Entry as KeyringEntry, Error as KeyringError};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn load_ai_settings(app: AppHandle) -> Result<AiSettings, String> {
    let (paths, conn) = open_library(&app)?;
    read_ai_settings(&conn, &paths)
}

#[tauri::command]
pub(crate) fn save_ai_settings(
    app: AppHandle,
    input: AiSettingsInput,
) -> Result<AiSettings, String> {
    let (paths, conn) = open_library(&app)?;
    save_ai_settings_with_conn(&conn, &paths, input)?;
    read_ai_settings(&conn, &paths)
}

#[tauri::command]
pub(crate) async fn test_ai_settings(app: AppHandle) -> Result<AiSettingsTestResult, String> {
    let config = {
        let (paths, conn) = open_library(&app)?;
        read_ai_runtime_config(&conn, &paths)?
    };
    let response = call_ai_json(
        &config,
        "你是一个只输出 JSON 的连接测试助手。",
        "请返回 {\"ok\":true,\"message\":\"connected\"}，不要输出其他文本。",
    )
    .await?;
    let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        return Err("AI 端点已响应，但返回内容不是预期 JSON。".to_string());
    }
    Ok(AiSettingsTestResult {
        ok: true,
        message: "AI 连接测试通过。".to_string(),
        model: config.model,
    })
}

#[tauri::command]
pub(crate) fn list_tags(app: AppHandle) -> Result<Vec<TagSummary>, String> {
    let (_, conn) = open_library(&app)?;
    read_tag_summaries(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn ai_classify_uncategorized(
    app: AppHandle,
    input: AiClassifyInput,
) -> Result<AiClassificationResult, String> {
    let config = {
        let (paths, conn) = open_library(&app)?;
        read_ai_runtime_config(&conn, &paths)?
    };
    let limit = input
        .limit
        .unwrap_or(AI_UNCATEGORIZED_DEFAULT_LIMIT)
        .clamp(1, 500);
    let (mut categories, notes) = {
        let (_, conn) = open_library(&app)?;
        (
            read_category_names(&conn).map_err(|error| error.to_string())?,
            read_ai_uncategorized_notes(&conn, limit).map_err(|error| error.to_string())?,
        )
    };
    if notes.is_empty() {
        return Ok(AiClassificationResult {
            scanned: 0,
            updated: 0,
            created_categories: Vec::new(),
            assignments: Vec::new(),
            message: "没有未分类收藏需要整理。".to_string(),
        });
    }

    let original_categories: HashSet<String> =
        categories.iter().map(|name| name.to_lowercase()).collect();
    let mut created_categories = Vec::new();
    let mut assignments = Vec::new();
    let mut updated = 0;

    for batch in notes.chunks(AI_CLASSIFY_BATCH_SIZE) {
        let batch_assignments = classify_notes_with_ai(&config, &categories, batch).await?;
        let applied = {
            let (_, mut conn) = open_library(&app)?;
            apply_ai_category_assignments(&mut conn, batch, batch_assignments)?
        };

        for assignment in applied {
            let category_key = assignment.category_name.to_lowercase();
            if !original_categories.contains(&category_key)
                && !created_categories
                    .iter()
                    .any(|name: &String| name.eq_ignore_ascii_case(&assignment.category_name))
            {
                created_categories.push(assignment.category_name.clone());
            }
            if !categories
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&assignment.category_name))
            {
                categories.push(assignment.category_name.clone());
            }
            updated += 1;
            assignments.push(assignment);
        }
    }

    Ok(AiClassificationResult {
        scanned: notes.len(),
        updated,
        created_categories,
        assignments,
        message: format!("AI 已整理 {updated} / {} 条未分类收藏。", notes.len()),
    })
}

#[tauri::command]
pub(crate) async fn ai_split_category(
    app: AppHandle,
    input: AiSplitCategoryInput,
) -> Result<AiClassificationResult, String> {
    let source_category = input.source_category_name.trim().to_string();
    let target_category = sanitize_category_name(&input.target_category_name)
        .or_else(|| sanitize_category_name(&input.query))
        .ok_or_else(|| "请输入要拆出的新分类。".to_string())?;
    let query = input.query.trim().to_string();
    if source_category.is_empty() {
        return Err("请选择来源大分类。".to_string());
    }
    if query.is_empty() {
        return Err("请输入筛选规则。".to_string());
    }
    if source_category.eq_ignore_ascii_case(&target_category) {
        return Err("新分类不能和来源分类同名。".to_string());
    }

    let config = {
        let (paths, conn) = open_library(&app)?;
        read_ai_runtime_config(&conn, &paths)?
    };
    let limit = input.limit.unwrap_or(160).clamp(1, 500);
    let notes = {
        let (_, conn) = open_library(&app)?;
        read_ai_notes_in_category(&conn, &source_category, limit)
            .map_err(|error| error.to_string())?
    };
    if notes.is_empty() {
        return Ok(AiClassificationResult {
            scanned: 0,
            updated: 0,
            created_categories: Vec::new(),
            assignments: Vec::new(),
            message: format!("「{source_category}」里没有可筛选的收藏。"),
        });
    }

    let mut assignments = Vec::new();
    let mut updated = 0;
    for batch in notes.chunks(AI_CLASSIFY_BATCH_SIZE) {
        let matched = filter_category_notes_with_ai(
            &config,
            &source_category,
            &target_category,
            &query,
            batch,
        )
        .await?;
        let applied = {
            let (_, mut conn) = open_library(&app)?;
            apply_ai_category_assignments(&mut conn, batch, matched)?
        };
        updated += applied.len();
        assignments.extend(applied);
    }

    let created_categories = if updated > 0 {
        vec![target_category.clone()]
    } else {
        Vec::new()
    };
    Ok(AiClassificationResult {
        scanned: notes.len(),
        updated,
        created_categories,
        assignments,
        message: format!("AI 已从「{source_category}」筛出 {updated} 条到「{target_category}」。"),
    })
}

#[tauri::command]
pub(crate) async fn ai_group_tags(
    app: AppHandle,
    input: AiTagGroupInput,
) -> Result<AiTagGroupResult, String> {
    let config = {
        let (paths, conn) = open_library(&app)?;
        read_ai_runtime_config(&conn, &paths)?
    };
    let limit = input
        .limit
        .unwrap_or(AI_TAG_GROUP_DEFAULT_LIMIT)
        .clamp(1, 600);
    let (categories, tags) = {
        let (_, conn) = open_library(&app)?;
        (
            read_category_names(&conn).map_err(|error| error.to_string())?,
            read_tag_summaries_limited(&conn, limit).map_err(|error| error.to_string())?,
        )
    };
    if tags.is_empty() {
        return Ok(AiTagGroupResult {
            scanned: 0,
            updated: 0,
            groups: Vec::new(),
            assignments: Vec::new(),
            message: "没有标签需要整理。".to_string(),
        });
    }

    let assignments = group_tags_with_ai(&config, &categories, &tags).await?;
    let updated = {
        let (_, mut conn) = open_library(&app)?;
        apply_ai_tag_groups(&mut conn, assignments.clone())?
    };
    let mut groups = Vec::new();
    for assignment in &assignments {
        if !groups
            .iter()
            .any(|name: &String| name.eq_ignore_ascii_case(&assignment.group_name))
        {
            groups.push(assignment.group_name.clone());
        }
    }

    Ok(AiTagGroupResult {
        scanned: tags.len(),
        updated,
        groups,
        assignments,
        message: format!("AI 已整理 {updated} 个标签分组。"),
    })
}

fn ai_api_key_id(profile_id: &str) -> String {
    let safe_profile_id: String = profile_id
        .chars()
        .map(|ch| {
            if matches!(ch, '\\' | '/' | ':') {
                '_'
            } else {
                ch
            }
        })
        .collect();
    format!("ai:{safe_profile_id}:api-key")
}

fn write_ai_secret(key_id: &str, secret: &str) -> Result<(), String> {
    ensure_keyring_store()?;
    KeyringEntry::new(AI_KEYRING_SERVICE, key_id)
        .map_err(|error| format!("创建 AI Keychain 条目失败：{error}"))?
        .set_password(secret)
        .map_err(|error| format!("写入 AI Keychain 失败：{error}"))
}

fn delete_ai_secret(key_id: &str) -> Result<(), String> {
    ensure_keyring_store()?;
    match KeyringEntry::new(AI_KEYRING_SERVICE, key_id)
        .map_err(|error| format!("创建 AI Keychain 条目失败：{error}"))?
        .delete_credential()
    {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(format!("删除 AI Keychain 条目失败：{error}")),
    }
}

fn read_ai_secret(key_id: &str) -> Result<String, String> {
    ensure_keyring_store()?;
    KeyringEntry::new(AI_KEYRING_SERVICE, key_id)
        .map_err(|error| format!("创建 AI Keychain 条目失败：{error}"))?
        .get_password()
        .map_err(|error| format!("读取 AI Keychain 失败：{error}"))
}

fn normalize_ai_provider(value: &str) -> String {
    match value.trim().to_lowercase().replace('-', "_").as_str() {
        "claude" | "anthropic" => "claude".to_string(),
        _ => AI_DEFAULT_PROVIDER.to_string(),
    }
}

fn read_ai_settings(conn: &Connection, paths: &LibraryPaths) -> Result<AiSettings, String> {
    let row = conn
        .query_row(
            "SELECT provider, base_url, model, api_key_key_id, api_key_storage, api_key_fallback,
                    temperature, max_tokens, updated_at
             FROM ai_settings
             WHERE id = ?1",
            params![AI_SETTINGS_ID],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some((
        provider,
        base_url,
        model,
        key_id,
        storage,
        fallback,
        temperature,
        max_tokens,
        updated_at,
    )) = row
    else {
        return Ok(AiSettings {
            provider: AI_DEFAULT_PROVIDER.to_string(),
            base_url: AI_DEFAULT_BASE_URL.to_string(),
            model: AI_DEFAULT_MODEL.to_string(),
            has_api_key: false,
            temperature: AI_DEFAULT_TEMPERATURE,
            max_tokens: AI_DEFAULT_MAX_TOKENS,
            updated_at: None,
        });
    };

    let key_id = key_id
        .or_else(|| Some(ai_api_key_id(&paths.profile.id)))
        .unwrap_or_default();
    let has_api_key = match storage.as_str() {
        "keychain" => !key_id.trim().is_empty() && read_ai_secret(&key_id).is_ok(),
        "sqlite_fallback" => !fallback.trim().is_empty(),
        _ => false,
    };

    Ok(AiSettings {
        provider: normalize_ai_provider(&provider),
        base_url: if base_url.trim().is_empty() {
            AI_DEFAULT_BASE_URL.to_string()
        } else {
            base_url
        },
        model: if model.trim().is_empty() {
            AI_DEFAULT_MODEL.to_string()
        } else {
            model
        },
        has_api_key,
        temperature,
        max_tokens,
        updated_at,
    })
}

fn save_ai_settings_with_conn(
    conn: &Connection,
    paths: &LibraryPaths,
    input: AiSettingsInput,
) -> Result<(), String> {
    let provider = normalize_ai_provider(&input.provider);
    let base_url = input.base_url.trim().trim_end_matches('/').to_string();
    let model = input.model.trim().to_string();
    if base_url.is_empty() {
        return Err("请输入 AI API 端点。".to_string());
    }
    if model.is_empty() {
        return Err("请输入模型名称。".to_string());
    }

    let existing = conn
        .query_row(
            "SELECT api_key_key_id, api_key_storage, api_key_fallback
             FROM ai_settings
             WHERE id = ?1",
            params![AI_SETTINGS_ID],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let default_key_id = ai_api_key_id(&paths.profile.id);
    let (mut key_id, mut storage, mut fallback) = existing.unwrap_or((
        Some(default_key_id.clone()),
        "none".to_string(),
        String::new(),
    ));
    if key_id.as_deref().unwrap_or_default().trim().is_empty() {
        key_id = Some(default_key_id);
    }

    if input.clear_api_key.unwrap_or(false) {
        let mut key_ids = Vec::new();
        if let Some(existing_key_id) = key_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            key_ids.push(existing_key_id.to_string());
        }
        let default_key_id = ai_api_key_id(&paths.profile.id);
        if !key_ids.iter().any(|value| value == &default_key_id) {
            key_ids.push(default_key_id);
        }
        for key_id in key_ids {
            if let Err(error) = delete_ai_secret(&key_id) {
                log::warn!("ai_keyring_delete_failed key_id={key_id} error={error}");
            }
        }
        key_id = None;
        storage = "none".to_string();
        fallback.clear();
    }

    if let Some(api_key) = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let next_key_id = key_id
            .clone()
            .unwrap_or_else(|| ai_api_key_id(&paths.profile.id));
        match write_ai_secret(&next_key_id, api_key) {
            Ok(()) => {
                storage = "keychain".to_string();
                fallback.clear();
                key_id = Some(next_key_id);
            }
            Err(error) => {
                log::warn!("ai_keyring_write_failed key_id={next_key_id} error={error}");
                storage = "sqlite_fallback".to_string();
                fallback = api_key.to_string();
                key_id = Some(next_key_id);
            }
        }
    }

    let temperature = input
        .temperature
        .unwrap_or(AI_DEFAULT_TEMPERATURE)
        .clamp(0.0, 1.2);
    let max_tokens = input
        .max_tokens
        .unwrap_or(AI_DEFAULT_MAX_TOKENS)
        .clamp(512, 16000);

    conn.execute(
        "INSERT INTO ai_settings (
            id, provider, base_url, model, api_key_key_id, api_key_storage,
            api_key_fallback, temperature, max_tokens
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            base_url = excluded.base_url,
            model = excluded.model,
            api_key_key_id = excluded.api_key_key_id,
            api_key_storage = excluded.api_key_storage,
            api_key_fallback = excluded.api_key_fallback,
            temperature = excluded.temperature,
            max_tokens = excluded.max_tokens,
            updated_at = CURRENT_TIMESTAMP",
        params![
            AI_SETTINGS_ID,
            provider,
            base_url,
            model,
            key_id.as_deref(),
            storage,
            fallback,
            temperature,
            max_tokens
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn read_ai_runtime_config(
    conn: &Connection,
    paths: &LibraryPaths,
) -> Result<AiRuntimeConfig, String> {
    let (provider, base_url, model, key_id, storage, fallback, temperature, max_tokens) = conn
        .query_row(
            "SELECT provider, base_url, model, api_key_key_id, api_key_storage, api_key_fallback,
                    temperature, max_tokens
             FROM ai_settings
             WHERE id = ?1",
            params![AI_SETTINGS_ID],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, f64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "请先在设置里保存 AI API 配置。".to_string())?;

    let key_id = key_id.unwrap_or_else(|| ai_api_key_id(&paths.profile.id));
    let api_key = match storage.as_str() {
        "keychain" => match read_ai_secret(&key_id) {
            Ok(secret) if !secret.trim().is_empty() => secret,
            Ok(_) => return Err("AI API Key 为空，请在设置里填写。".to_string()),
            Err(error) => {
                log::warn!("ai_keyring_read_failed key_id={key_id} error={error}");
                return Err("AI API Key 读取失败，请在设置里重新保存。".to_string());
            }
        },
        "sqlite_fallback" if !fallback.trim().is_empty() => fallback,
        _ => return Err("AI API Key 为空，请在设置里填写。".to_string()),
    };

    let base_url = base_url.trim().trim_end_matches('/').to_string();
    let model = model.trim().to_string();
    if base_url.is_empty() || model.is_empty() {
        return Err("AI API 端点或模型为空，请在设置里补全。".to_string());
    }

    Ok(AiRuntimeConfig {
        provider: normalize_ai_provider(&provider),
        base_url,
        model,
        api_key,
        temperature: temperature.clamp(0.0, 1.2),
        max_tokens: max_tokens.clamp(512, 16000),
    })
}

async fn call_ai_json(config: &AiRuntimeConfig, system: &str, user: &str) -> Result<Value, String> {
    let text = call_ai_text(config, system, user).await?;
    extract_json_object(&text)
        .ok_or_else(|| format!("AI 返回内容不是 JSON：{}", truncate_chars(text.trim(), 180)))
}

async fn call_ai_text(
    config: &AiRuntimeConfig,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("创建 AI HTTP 客户端失败：{error}"))?;
    if config.provider == "claude" {
        call_claude_text(&client, config, system, user).await
    } else {
        call_openai_compatible_text(&client, config, system, user).await
    }
}

async fn call_openai_compatible_text(
    client: &Client,
    config: &AiRuntimeConfig,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let url = ai_endpoint_url(&config.base_url, "chat/completions");
    let body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "temperature": config.temperature,
        "max_tokens": config.max_tokens
    });
    let response = client
        .post(url)
        .bearer_auth(config.api_key.trim())
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|error| format!("AI 请求失败：{error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取 AI 响应失败：{error}"))?;
    if !status.is_success() {
        return Err(format!(
            "AI 请求返回 HTTP {}：{}",
            status.as_u16(),
            truncate_chars(&text, 260)
        ));
    }
    let value: Value =
        serde_json::from_str(&text).map_err(|error| format!("AI 响应 JSON 解析失败：{error}"))?;
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "AI 响应缺少 message.content：{}",
                truncate_chars(&text, 260)
            )
        })
}

async fn call_claude_text(
    client: &Client,
    config: &AiRuntimeConfig,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let url = ai_endpoint_url(&config.base_url, "messages");
    let body = json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "temperature": config.temperature,
        "system": system,
        "messages": [
            { "role": "user", "content": user }
        ]
    });
    let response = client
        .post(url)
        .header("x-api-key", config.api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|error| format!("Claude 请求失败：{error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取 Claude 响应失败：{error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Claude 请求返回 HTTP {}：{}",
            status.as_u16(),
            truncate_chars(&text, 260)
        ));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("Claude 响应 JSON 解析失败：{error}"))?;
    value
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "Claude 响应缺少 content.text：{}",
                truncate_chars(&text, 260)
            )
        })
}

fn ai_endpoint_url(base_url: &str, suffix: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with(suffix) {
        base.to_string()
    } else {
        format!("{base}/{suffix}")
    }
}

fn extract_json_object(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim()
        .trim_end_matches("```")
        .trim();
    if let Ok(value) = serde_json::from_str::<Value>(without_fence) {
        return Some(value);
    }
    let start = without_fence.find('{')?;
    let end = without_fence.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&without_fence[start..=end]).ok()
}

fn read_category_names(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name
         FROM categories
         WHERE parent_id IS NULL
         ORDER BY sort_order ASC, name ASC",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn read_ai_uncategorized_notes(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<AiNoteDigest>> {
    read_ai_notes_with_sql(
        conn,
        "SELECT n.id, COALESCE(n.title, ''), COALESCE(n.excerpt, ''),
                COALESCE(n.content, ''), COALESCE(n.author_name, ''), c.name
         FROM notes n
         LEFT JOIN categories c ON c.id = n.category_id
         WHERE n.category_id IS NULL
           AND COALESCE(n.remote_status, 'available') = 'available'
         ORDER BY
            CASE WHEN n.collected_at IS NULL OR n.collected_at = '' THEN 1 ELSE 0 END ASC,
            datetime(n.collected_at) DESC,
            datetime(n.last_seen_at) DESC
         LIMIT ?1",
        params![limit as i64],
    )
}

fn read_ai_notes_in_category(
    conn: &Connection,
    category_name: &str,
    limit: usize,
) -> rusqlite::Result<Vec<AiNoteDigest>> {
    read_ai_notes_with_sql(
        conn,
        "SELECT n.id, COALESCE(n.title, ''), COALESCE(n.excerpt, ''),
                COALESCE(n.content, ''), COALESCE(n.author_name, ''), c.name
         FROM notes n
         INNER JOIN categories c ON c.id = n.category_id
         WHERE c.name = ?1
           AND COALESCE(n.remote_status, 'available') = 'available'
         ORDER BY
            CASE WHEN n.collected_at IS NULL OR n.collected_at = '' THEN 1 ELSE 0 END ASC,
            datetime(n.collected_at) DESC,
            datetime(n.last_seen_at) DESC
         LIMIT ?2",
        params![category_name, limit as i64],
    )
}

fn read_ai_notes_with_sql<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> rusqlite::Result<Vec<AiNoteDigest>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |row| {
        Ok(AiNoteDigest {
            id: row.get(0)?,
            title: row.get(1)?,
            excerpt: row.get(2)?,
            content: row.get(3)?,
            author_name: row.get(4)?,
            category_name: row.get(5)?,
            tags: Vec::new(),
        })
    })?;
    let mut notes = Vec::new();
    for row in rows {
        let mut note = row?;
        note.tags = read_tags(conn, &note.id)?;
        notes.push(note);
    }
    Ok(notes)
}

fn read_tag_summaries(conn: &Connection) -> rusqlite::Result<Vec<TagSummary>> {
    read_tag_summaries_limited(conn, i64::MAX as usize)
}

fn read_tag_summaries_limited(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<TagSummary>> {
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(nt.note_id) AS usage_count, t.kind, t.ai_group
         FROM tags t
         LEFT JOIN note_tags nt ON nt.tag_id = t.id
         GROUP BY t.id, t.name, t.kind, t.ai_group
         ORDER BY usage_count DESC, t.name ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(TagSummary {
            name: row.get(0)?,
            count: row.get(1)?,
            kind: row.get(2)?,
            group_name: row.get(3)?,
        })
    })?;
    rows.collect()
}

async fn classify_notes_with_ai(
    config: &AiRuntimeConfig,
    categories: &[String],
    notes: &[AiNoteDigest],
) -> Result<Vec<AiAssignmentResult>, String> {
    let system = "你是本地收藏管理软件的分类助手。只返回 JSON，不要 Markdown。";
    let user = json!({
        "task": "把未分类的小红书收藏归入大类。先优先使用 existingCategories 中最合适的大类；如果没有合适的大类，创建一个新的中文大类。大类要稳定、宽泛、短，避免为单篇帖子创建过细分类。不要返回“未分类”。",
        "existingCategories": categories,
        "notes": notes.iter().map(ai_note_payload).collect::<Vec<_>>(),
        "outputSchema": {
            "assignments": [
                {
                    "noteId": "输入中的 id",
                    "categoryName": "已有或新建的大类名",
                    "confidence": 0.0,
                    "reason": "一句简短理由"
                }
            ]
        }
    });
    let value = call_ai_json(config, system, &user.to_string()).await?;
    parse_ai_category_assignments(&value, notes, None)
}

async fn filter_category_notes_with_ai(
    config: &AiRuntimeConfig,
    source_category: &str,
    target_category: &str,
    query: &str,
    notes: &[AiNoteDigest],
) -> Result<Vec<AiAssignmentResult>, String> {
    let system = "你是本地收藏管理软件的分类筛选助手。只返回 JSON，不要 Markdown。";
    let user = json!({
        "task": "从一个大分类中筛出符合用户规则的收藏。只有明显相关时 move 才为 true；模糊、只是沾边、或无法判断时为 false。",
        "sourceCategory": source_category,
        "targetCategory": target_category,
        "userRule": query,
        "notes": notes.iter().map(ai_note_payload).collect::<Vec<_>>(),
        "outputSchema": {
            "matches": [
                {
                    "noteId": "输入中的 id",
                    "move": true,
                    "confidence": 0.0,
                    "reason": "一句简短理由"
                }
            ]
        }
    });
    let value = call_ai_json(config, system, &user.to_string()).await?;
    parse_ai_filter_assignments(&value, notes, target_category)
}

async fn group_tags_with_ai(
    config: &AiRuntimeConfig,
    categories: &[String],
    tags: &[TagSummary],
) -> Result<Vec<AiTagAssignmentResult>, String> {
    let system = "你是本地收藏管理软件的标签整理助手。只返回 JSON，不要 Markdown。";
    let user = json!({
        "task": "根据已有大类和标签文本，把标签归入便于检索的标签组。优先使用 existingCategories 作为 groupName；如果不合适，可以创建短中文组名。标签组应稳定、宽泛、数量不要太多。",
        "existingCategories": categories,
        "tags": tags.iter().map(|tag| json!({
            "name": tag.name,
            "count": tag.count,
            "kind": tag.kind
        })).collect::<Vec<_>>(),
        "outputSchema": {
            "assignments": [
                {
                    "tag": "输入中的 name",
                    "groupName": "标签组名",
                    "confidence": 0.0,
                    "reason": "一句简短理由"
                }
            ]
        }
    });
    let value = call_ai_json(config, system, &user.to_string()).await?;
    parse_ai_tag_assignments(&value, tags)
}

fn ai_note_payload(note: &AiNoteDigest) -> Value {
    let body = if note.content.trim().is_empty() {
        note.excerpt.trim()
    } else {
        note.content.trim()
    };
    json!({
        "id": note.id,
        "title": truncate_chars(&note.title, 140),
        "author": truncate_chars(&note.author_name, 80),
        "category": note.category_name,
        "text": truncate_chars(body, AI_NOTE_TEXT_LIMIT),
        "tags": note.tags
    })
}

fn parse_ai_category_assignments(
    value: &Value,
    notes: &[AiNoteDigest],
    fixed_category: Option<&str>,
) -> Result<Vec<AiAssignmentResult>, String> {
    let note_titles: HashMap<&str, &str> = notes
        .iter()
        .map(|note| (note.id.as_str(), note.title.as_str()))
        .collect();
    let Some(items) = value.get("assignments").and_then(Value::as_array) else {
        return Err("AI 返回缺少 assignments。".to_string());
    };
    let mut assignments = Vec::new();
    for item in items {
        let Some(note_id) = item.get("noteId").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if !note_titles.contains_key(note_id) {
            continue;
        }
        let raw_category = fixed_category.map(str::to_string).or_else(|| {
            item.get("categoryName")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        let Some(category_name) = raw_category.as_deref().and_then(sanitize_category_name) else {
            continue;
        };
        assignments.push(AiAssignmentResult {
            note_id: note_id.to_string(),
            title: note_titles
                .get(note_id)
                .copied()
                .unwrap_or_default()
                .to_string(),
            category_name,
            confidence: item
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.75)
                .clamp(0.0, 1.0),
            reason: item
                .get("reason")
                .and_then(Value::as_str)
                .map(|text| truncate_chars(text, 80))
                .unwrap_or_default(),
        });
    }
    Ok(assignments)
}

fn parse_ai_filter_assignments(
    value: &Value,
    notes: &[AiNoteDigest],
    target_category: &str,
) -> Result<Vec<AiAssignmentResult>, String> {
    let note_titles: HashMap<&str, &str> = notes
        .iter()
        .map(|note| (note.id.as_str(), note.title.as_str()))
        .collect();
    let Some(items) = value
        .get("matches")
        .or_else(|| value.get("assignments"))
        .and_then(Value::as_array)
    else {
        return Err("AI 返回缺少 matches。".to_string());
    };
    let mut assignments = Vec::new();
    for item in items {
        if !item.get("move").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let Some(note_id) = item.get("noteId").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if !note_titles.contains_key(note_id) {
            continue;
        }
        assignments.push(AiAssignmentResult {
            note_id: note_id.to_string(),
            title: note_titles
                .get(note_id)
                .copied()
                .unwrap_or_default()
                .to_string(),
            category_name: target_category.to_string(),
            confidence: item
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.75)
                .clamp(0.0, 1.0),
            reason: item
                .get("reason")
                .and_then(Value::as_str)
                .map(|text| truncate_chars(text, 80))
                .unwrap_or_default(),
        });
    }
    Ok(assignments)
}

fn parse_ai_tag_assignments(
    value: &Value,
    tags: &[TagSummary],
) -> Result<Vec<AiTagAssignmentResult>, String> {
    let known: HashSet<&str> = tags.iter().map(|tag| tag.name.as_str()).collect();
    let Some(items) = value.get("assignments").and_then(Value::as_array) else {
        return Err("AI 返回缺少 assignments。".to_string());
    };
    let mut assignments = Vec::new();
    for item in items {
        let Some(tag) = item.get("tag").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if !known.contains(tag) {
            continue;
        }
        let Some(group_name) = item
            .get("groupName")
            .and_then(Value::as_str)
            .and_then(sanitize_category_name)
        else {
            continue;
        };
        assignments.push(AiTagAssignmentResult {
            tag: tag.to_string(),
            group_name,
            confidence: item
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.75)
                .clamp(0.0, 1.0),
            reason: item
                .get("reason")
                .and_then(Value::as_str)
                .map(|text| truncate_chars(text, 80))
                .unwrap_or_default(),
        });
    }
    Ok(assignments)
}

fn apply_ai_category_assignments(
    conn: &mut Connection,
    notes: &[AiNoteDigest],
    assignments: Vec<AiAssignmentResult>,
) -> Result<Vec<AiAssignmentResult>, String> {
    let note_ids: HashSet<&str> = notes.iter().map(|note| note.id.as_str()).collect();
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let mut applied = Vec::new();
    for assignment in assignments {
        if !note_ids.contains(assignment.note_id.as_str()) {
            continue;
        }
        let Some(category_name) = sanitize_category_name(&assignment.category_name) else {
            continue;
        };
        let category_id = upsert_optional_category(&transaction, &category_name)
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE notes
                 SET category_id = ?1,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?2",
                params![category_id.as_deref(), &assignment.note_id],
            )
            .map_err(|error| error.to_string())?;
        applied.push(AiAssignmentResult {
            category_name,
            ..assignment
        });
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(applied)
}

fn apply_ai_tag_groups(
    conn: &mut Connection,
    assignments: Vec<AiTagAssignmentResult>,
) -> Result<usize, String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let mut updated = 0;
    for assignment in assignments {
        let Some(group_name) = sanitize_category_name(&assignment.group_name) else {
            continue;
        };
        updated += transaction
            .execute(
                "UPDATE tags
                 SET ai_group = ?1,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE name = ?2",
                params![group_name, assignment.tag],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(updated)
}

fn sanitize_category_name(raw: &str) -> Option<String> {
    let mut cleaned: String = raw
        .trim()
        .trim_matches(['"', '\'', '`', '#', '，', ',', '。', '.', ' '])
        .chars()
        .map(|ch| {
            if matches!(ch, '\r' | '\n' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect();
    while cleaned.contains("  ") {
        cleaned = cleaned.replace("  ", " ");
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned.eq_ignore_ascii_case("未分类") || cleaned.len() > 48 {
        return None;
    }
    Some(cleaned.chars().take(24).collect())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            output.push('…');
            break;
        }
        output.push(ch);
    }
    output
}
