use crate::constants::*;
use crate::models::*;
use crate::secrets::ensure_keyring_store;
use crate::storage::{
    app_data_dir, is_noise_tag_name, normalize_tag_alias_key, open_library, read_tags,
    upsert_optional_category, upsert_tag_with_kind,
};
use crate::utils::display_path;
use keyring_core::{Entry as KeyringEntry, Error as KeyringError};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::{Client, StatusCode};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const AI_PROMPTS_YAML: &str = include_str!("../prompts/ai.yaml");
const AI_PROMPT_FILE: &str = "ai.yaml";
const AI_CANCELLED_MESSAGE: &str = "AI 任务已停止。已写入的批次会保留，后续内容未处理。";
const TAG_CATEGORY_AUTO_CONFIDENCE_THRESHOLD: f64 = 0.60;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AiPromptCatalog {
    version: u16,
    prompts: AiPromptSet,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AiPromptSet {
    test_connection: AiTextPrompt,
    classify_uncategorized: AiTaskPrompt,
    split_category: AiTaskPrompt,
    group_tags: AiTaskPrompt,
    tag_merge_suggestions: AiTagMergePrompt,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AiTextPrompt {
    system: String,
    user: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AiTaskPrompt {
    system: String,
    task: String,
    output_schema: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AiTagMergePrompt {
    system: String,
    task: String,
    #[serde(default)]
    rules: Vec<String>,
    return_json_shape: Value,
}

#[derive(Debug, Clone)]
struct TagCategoryHint {
    candidates: Vec<(String, i64)>,
    categorized_total: i64,
}

fn default_ai_prompt_catalog() -> Result<&'static AiPromptCatalog, String> {
    static CATALOG: OnceLock<Result<AiPromptCatalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| parse_ai_prompt_catalog(AI_PROMPTS_YAML))
        .as_ref()
        .map_err(Clone::clone)
}

fn parse_ai_prompt_catalog(yaml: &str) -> Result<AiPromptCatalog, String> {
    let catalog: AiPromptCatalog =
        yaml_serde::from_str(yaml).map_err(|error| format!("AI prompt YAML 解析失败：{error}"))?;
    if catalog.version != 1 {
        return Err(format!("AI prompt YAML 版本不支持：{}", catalog.version));
    }
    Ok(catalog)
}

fn active_ai_prompt_catalog(app: &AppHandle) -> Result<AiPromptCatalog, String> {
    let path = ai_prompt_path(app)?;
    if path.exists() {
        match fs::read_to_string(&path)
            .map_err(|error| format!("读取 AI prompt 文件失败：{error}"))
            .and_then(|raw| parse_ai_prompt_catalog(&raw))
        {
            Ok(catalog) => return Ok(catalog),
            Err(error) => {
                log::warn!(
                    "ai_prompt_custom_invalid path={} error={}",
                    display_path(path),
                    error
                );
            }
        }
    }
    default_ai_prompt_catalog().cloned()
}

fn ai_prompt_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("prompts").join(AI_PROMPT_FILE))
}

fn ai_prompt_label(key: &str) -> &'static str {
    match key {
        "test_connection" => "连接测试",
        "classify_uncategorized" => "未分类归类",
        "split_category" => "拆分分类",
        "group_tags" => "标签归属",
        "tag_merge_suggestions" => "标签去重",
        _ => "Prompt",
    }
}

fn catalog_to_editor_items(catalog: &AiPromptCatalog) -> Result<Vec<AiPromptEditorItem>, String> {
    Ok(vec![
        AiPromptEditorItem {
            key: "test_connection".to_string(),
            label: ai_prompt_label("test_connection").to_string(),
            system: catalog.prompts.test_connection.system.clone(),
            user: Some(catalog.prompts.test_connection.user.clone()),
            task: None,
            rules: Vec::new(),
            schema_kind: None,
            schema_text: None,
        },
        task_prompt_editor_item(
            "classify_uncategorized",
            &catalog.prompts.classify_uncategorized,
        )?,
        task_prompt_editor_item("split_category", &catalog.prompts.split_category)?,
        task_prompt_editor_item("group_tags", &catalog.prompts.group_tags)?,
        AiPromptEditorItem {
            key: "tag_merge_suggestions".to_string(),
            label: ai_prompt_label("tag_merge_suggestions").to_string(),
            system: catalog.prompts.tag_merge_suggestions.system.clone(),
            user: None,
            task: Some(catalog.prompts.tag_merge_suggestions.task.clone()),
            rules: catalog.prompts.tag_merge_suggestions.rules.clone(),
            schema_kind: Some("return_json_shape".to_string()),
            schema_text: Some(pretty_json(
                &catalog.prompts.tag_merge_suggestions.return_json_shape,
            )?),
        },
    ])
}

fn task_prompt_editor_item(key: &str, prompt: &AiTaskPrompt) -> Result<AiPromptEditorItem, String> {
    Ok(AiPromptEditorItem {
        key: key.to_string(),
        label: ai_prompt_label(key).to_string(),
        system: prompt.system.clone(),
        user: None,
        task: Some(prompt.task.clone()),
        rules: Vec::new(),
        schema_kind: Some("output_schema".to_string()),
        schema_text: Some(pretty_json(&prompt.output_schema)?),
    })
}

fn pretty_json(value: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| format!("格式化 prompt schema 失败：{error}"))
}

fn editor_items_to_catalog(items: Vec<AiPromptEditorItem>) -> Result<AiPromptCatalog, String> {
    let mut by_key = items
        .into_iter()
        .map(|item| (item.key.clone(), item))
        .collect::<HashMap<_, _>>();
    let test_connection = by_key
        .remove("test_connection")
        .ok_or_else(|| "缺少连接测试 prompt。".to_string())?;
    Ok(AiPromptCatalog {
        version: 1,
        prompts: AiPromptSet {
            test_connection: AiTextPrompt {
                system: required_prompt_text(&test_connection.system, "连接测试 system")?,
                user: required_prompt_text(
                    test_connection.user.as_deref().unwrap_or_default(),
                    "连接测试 user",
                )?,
            },
            classify_uncategorized: task_prompt_from_editor(
                by_key.remove("classify_uncategorized"),
                "未分类归类",
            )?,
            split_category: task_prompt_from_editor(by_key.remove("split_category"), "拆分分类")?,
            group_tags: task_prompt_from_editor(by_key.remove("group_tags"), "标签归属")?,
            tag_merge_suggestions: tag_merge_prompt_from_editor(
                by_key.remove("tag_merge_suggestions"),
            )?,
        },
    })
}

fn task_prompt_from_editor(
    item: Option<AiPromptEditorItem>,
    label: &str,
) -> Result<AiTaskPrompt, String> {
    let item = item.ok_or_else(|| format!("缺少 {label} prompt。"))?;
    Ok(AiTaskPrompt {
        system: required_prompt_text(&item.system, &format!("{label} system"))?,
        task: required_prompt_text(
            item.task.as_deref().unwrap_or_default(),
            &format!("{label} task"),
        )?,
        output_schema: parse_prompt_schema(item.schema_text.as_deref(), label)?,
    })
}

fn tag_merge_prompt_from_editor(
    item: Option<AiPromptEditorItem>,
) -> Result<AiTagMergePrompt, String> {
    let item = item.ok_or_else(|| "缺少标签去重 prompt。".to_string())?;
    Ok(AiTagMergePrompt {
        system: required_prompt_text(&item.system, "标签去重 system")?,
        task: required_prompt_text(item.task.as_deref().unwrap_or_default(), "标签去重 task")?,
        rules: item
            .rules
            .into_iter()
            .map(|rule| rule.trim().to_string())
            .filter(|rule| !rule.is_empty())
            .collect(),
        return_json_shape: parse_prompt_schema(item.schema_text.as_deref(), "标签去重")?,
    })
}

fn required_prompt_text(value: &str, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} 不能为空。"))
    } else {
        Ok(value.to_string())
    }
}

fn parse_prompt_schema(value: Option<&str>, label: &str) -> Result<Value, String> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        return Err(format!("{label} schema 不能为空。"));
    }
    serde_json::from_str(value).map_err(|error| format!("{label} schema 必须是合法 JSON：{error}"))
}

fn load_ai_prompt_settings_from_path(
    path: PathBuf,
    custom_error: Option<String>,
) -> Result<AiPromptSettings, String> {
    let (catalog, is_custom, validation_error) = if path.exists() {
        match fs::read_to_string(&path)
            .map_err(|error| format!("读取 AI prompt 文件失败：{error}"))
            .and_then(|raw| parse_ai_prompt_catalog(&raw))
        {
            Ok(catalog) => (catalog, true, custom_error),
            Err(error) => (default_ai_prompt_catalog()?.clone(), true, Some(error)),
        }
    } else {
        (default_ai_prompt_catalog()?.clone(), false, custom_error)
    };
    Ok(AiPromptSettings {
        path: display_path(path),
        is_custom,
        validation_error,
        prompts: catalog_to_editor_items(&catalog)?,
    })
}

#[tauri::command]
pub(crate) fn load_ai_prompt_settings(app: AppHandle) -> Result<AiPromptSettings, String> {
    load_ai_prompt_settings_from_path(ai_prompt_path(&app)?, None)
}

#[tauri::command]
pub(crate) fn save_ai_prompt_settings(
    app: AppHandle,
    input: AiPromptSettingsInput,
) -> Result<AiPromptSettings, String> {
    let catalog = editor_items_to_catalog(input.prompts)?;
    let yaml = yaml_serde::to_string(&catalog)
        .map_err(|error| format!("生成 AI prompt YAML 失败：{error}"))?;
    let path = ai_prompt_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建 AI prompt 目录失败：{error}"))?;
    }
    fs::write(&path, yaml).map_err(|error| format!("写入 AI prompt 文件失败：{error}"))?;
    load_ai_prompt_settings_from_path(path, None)
}

#[tauri::command]
pub(crate) fn reset_ai_prompt_settings(app: AppHandle) -> Result<AiPromptSettings, String> {
    let path = ai_prompt_path(&app)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("删除自定义 AI prompt 失败：{error}"))?;
    }
    load_ai_prompt_settings_from_path(path, None)
}

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
    let prompt_catalog = active_ai_prompt_catalog(&app)?;
    let prompt = &prompt_catalog.prompts.test_connection;
    let response = call_ai_json(&config, &prompt.system, &prompt.user).await?;
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
pub(crate) fn clear_ai_tag_groups(app: AppHandle) -> Result<TagGroupClearResult, String> {
    let (_, conn) = open_library(&app)?;
    let scanned = conn
        .query_row(
            "SELECT COUNT(*)
             FROM tags
             WHERE ai_group IS NOT NULL
               AND TRIM(ai_group) <> ''",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        .max(0) as usize;
    let cleared = conn
        .execute(
            "UPDATE tags
             SET ai_group = NULL,
                 updated_at = CURRENT_TIMESTAMP
             WHERE ai_group IS NOT NULL
               AND TRIM(ai_group) <> ''",
            [],
        )
        .map_err(|error| error.to_string())?;
    let message = if cleared == 0 {
        "当前没有标签分类需要清空。".to_string()
    } else {
        format!("已清空 {cleared} 个标签分类归属。")
    };
    emit_library_changed(&app, message.clone());
    Ok(TagGroupClearResult {
        scanned,
        cleared,
        message,
    })
}

#[tauri::command]
pub(crate) fn cancel_ai_task(task: Option<String>) -> Result<(), String> {
    request_ai_task_cancel();
    log::info!(
        "ai_cancel_requested task={}",
        task.as_deref().unwrap_or("ai")
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn ai_classify_uncategorized(
    app: AppHandle,
    input: AiClassifyInput,
) -> Result<AiClassificationResult, String> {
    reset_ai_task_cancel();
    let prompt_catalog = active_ai_prompt_catalog(&app)?;
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
    let task = "classify_uncategorized";
    if notes.is_empty() {
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "completed".to_string(),
                label: "AI 自动分类完成".to_string(),
                detail: "没有未分类收藏需要整理。".to_string(),
                planned: 0,
                scanned: 0,
                updated: 0,
                failed: 0,
                skipped: 0,
                progress: 100,
                indeterminate: false,
                error: None,
            },
        );
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
    let mut scanned = 0;
    let mut failed = 0;
    let batch_count = notes.len().div_ceil(AI_CLASSIFY_BATCH_SIZE);

    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "preparing".to_string(),
            label: "AI 自动分类".to_string(),
            detail: format!("已找到 {} 条未分类收藏，准备分批整理。", notes.len()),
            planned: notes.len(),
            scanned,
            updated,
            failed,
            skipped: 0,
            progress: 2,
            indeterminate: false,
            error: None,
        },
    );

    for (batch_index, batch) in notes.chunks(AI_CLASSIFY_BATCH_SIZE).enumerate() {
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "AI 自动分类已停止",
                notes.len(),
                scanned,
                updated,
                failed,
                scanned.saturating_sub(updated),
            ));
        }
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "requesting".to_string(),
                label: "AI 自动分类".to_string(),
                detail: format!(
                    "正在请求第 {} / {batch_count} 批，本批 {} 条。",
                    batch_index + 1,
                    batch.len()
                ),
                planned: notes.len(),
                scanned,
                updated,
                failed,
                skipped: scanned.saturating_sub(updated),
                progress: ai_progress_percent(scanned, notes.len()).max(3),
                indeterminate: false,
                error: None,
            },
        );
        let batch_assignments = match classify_notes_with_ai(
            &config,
            &prompt_catalog.prompts.classify_uncategorized,
            &categories,
            batch,
        )
        .await
        {
            Ok(assignments) => assignments,
            Err(error) if should_retry_smaller_ai_batch(&error, batch.len()) => {
                log::warn!(
                    "ai_classify_retry_smaller batch_index={} batch_size={} error={}",
                    batch_index + 1,
                    batch.len(),
                    truncate_chars(&error, 220)
                );
                emit_ai_progress(
                    &app,
                    AiJobProgress {
                        task: task.to_string(),
                        phase: "retrying".to_string(),
                        label: "AI 自动分类".to_string(),
                        detail: format!("第 {} 批响应被截断，正在缩小批次重试。", batch_index + 1),
                        planned: notes.len(),
                        scanned,
                        updated,
                        failed,
                        skipped: scanned.saturating_sub(updated),
                        progress: ai_progress_percent(scanned, notes.len()).max(3),
                        indeterminate: false,
                        error: None,
                    },
                );
                match classify_notes_with_smaller_batches(
                    &config,
                    &prompt_catalog.prompts.classify_uncategorized,
                    &categories,
                    batch,
                )
                .await
                {
                    Ok(assignments) => assignments,
                    Err(retry_error) => {
                        if ai_task_cancel_requested() {
                            return Err(emit_ai_cancelled_progress(
                                &app,
                                task,
                                "AI 自动分类已停止",
                                notes.len(),
                                scanned,
                                updated,
                                failed,
                                scanned.saturating_sub(updated),
                            ));
                        }
                        failed += batch.len();
                        let combined_error = format!(
                            "{error}；自动缩小到每批 {} 条后仍失败：{retry_error}",
                            AI_CLASSIFY_RETRY_BATCH_SIZE
                        );
                        emit_ai_failed_progress(
                            &app,
                            task,
                            "AI 自动分类失败",
                            combined_error.clone(),
                            notes.len(),
                            scanned,
                            updated,
                            failed,
                            scanned.saturating_sub(updated),
                        );
                        return Err(combined_error);
                    }
                }
            }
            Err(error) => {
                if ai_task_cancel_requested() {
                    return Err(emit_ai_cancelled_progress(
                        &app,
                        task,
                        "AI 自动分类已停止",
                        notes.len(),
                        scanned,
                        updated,
                        failed,
                        scanned.saturating_sub(updated),
                    ));
                }
                failed += batch.len();
                emit_ai_failed_progress(
                    &app,
                    task,
                    "AI 自动分类失败",
                    error.clone(),
                    notes.len(),
                    scanned,
                    updated,
                    failed,
                    scanned.saturating_sub(updated),
                );
                return Err(error);
            }
        };
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "AI 自动分类已停止",
                notes.len(),
                scanned,
                updated,
                failed,
                scanned.saturating_sub(updated),
            ));
        }
        let filtered_assignments = batch_assignments
            .into_iter()
            .filter(|assignment| {
                categories
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&assignment.category_name))
                    || assignment.confidence >= AI_MIN_NEW_CATEGORY_CONFIDENCE
            })
            .collect::<Vec<_>>();
        let applied = match open_library(&app).and_then(|(_, mut conn)| {
            apply_ai_category_assignments(&mut conn, batch, filtered_assignments)
        }) {
            Ok(applied) => applied,
            Err(error) => {
                failed += batch.len();
                emit_ai_failed_progress(
                    &app,
                    task,
                    "AI 自动分类失败",
                    error.clone(),
                    notes.len(),
                    scanned,
                    updated,
                    failed,
                    scanned.saturating_sub(updated),
                );
                return Err(error);
            }
        };

        let updated_before = updated;
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
        scanned += batch.len();
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "batch_completed".to_string(),
                label: "AI 自动分类".to_string(),
                detail: format!(
                    "已完成第 {} / {batch_count} 批，已归类 {updated} 条。",
                    batch_index + 1
                ),
                planned: notes.len(),
                scanned,
                updated,
                failed,
                skipped: scanned.saturating_sub(updated),
                progress: ai_progress_percent(scanned, notes.len()),
                indeterminate: false,
                error: None,
            },
        );
        if updated > updated_before {
            emit_library_changed(
                &app,
                format!(
                    "AI 已写入第 {} / {batch_count} 批，已归类 {updated} 条。",
                    batch_index + 1
                ),
            );
        }
    }

    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "completed".to_string(),
            label: "AI 自动分类完成".to_string(),
            detail: format!("AI 已整理 {updated} / {} 条未分类收藏。", notes.len()),
            planned: notes.len(),
            scanned: notes.len(),
            updated,
            failed,
            skipped: notes.len().saturating_sub(updated),
            progress: 100,
            indeterminate: false,
            error: None,
        },
    );

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
    reset_ai_task_cancel();
    let prompt_catalog = active_ai_prompt_catalog(&app)?;
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
    let task = "split_category";
    if notes.is_empty() {
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "completed".to_string(),
                label: "拆分分类完成".to_string(),
                detail: format!("「{source_category}」里没有可筛选的收藏。"),
                planned: 0,
                scanned: 0,
                updated: 0,
                failed: 0,
                skipped: 0,
                progress: 100,
                indeterminate: false,
                error: None,
            },
        );
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
    let mut scanned = 0;
    let mut failed = 0;
    let batch_count = notes.len().div_ceil(AI_CLASSIFY_BATCH_SIZE);
    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "preparing".to_string(),
            label: "拆分当前分类".to_string(),
            detail: format!("准备从「{source_category}」筛出「{target_category}」。"),
            planned: notes.len(),
            scanned,
            updated,
            failed,
            skipped: 0,
            progress: 2,
            indeterminate: false,
            error: None,
        },
    );
    for (batch_index, batch) in notes.chunks(AI_CLASSIFY_BATCH_SIZE).enumerate() {
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "拆分分类已停止",
                notes.len(),
                scanned,
                updated,
                failed,
                scanned.saturating_sub(updated),
            ));
        }
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "requesting".to_string(),
                label: "拆分当前分类".to_string(),
                detail: format!(
                    "正在筛选第 {} / {batch_count} 批，本批 {} 条。",
                    batch_index + 1,
                    batch.len()
                ),
                planned: notes.len(),
                scanned,
                updated,
                failed,
                skipped: scanned.saturating_sub(updated),
                progress: ai_progress_percent(scanned, notes.len()).max(3),
                indeterminate: false,
                error: None,
            },
        );
        let matched = match filter_category_notes_with_ai(
            &config,
            &prompt_catalog.prompts.split_category,
            &source_category,
            &target_category,
            &query,
            batch,
        )
        .await
        {
            Ok(assignments) => assignments,
            Err(error) => {
                if ai_task_cancel_requested() {
                    return Err(emit_ai_cancelled_progress(
                        &app,
                        task,
                        "拆分分类已停止",
                        notes.len(),
                        scanned,
                        updated,
                        failed,
                        scanned.saturating_sub(updated),
                    ));
                }
                failed += batch.len();
                emit_ai_failed_progress(
                    &app,
                    task,
                    "拆分分类失败",
                    error.clone(),
                    notes.len(),
                    scanned,
                    updated,
                    failed,
                    scanned.saturating_sub(updated),
                );
                return Err(error);
            }
        };
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "拆分分类已停止",
                notes.len(),
                scanned,
                updated,
                failed,
                scanned.saturating_sub(updated),
            ));
        }
        let applied = match open_library(&app)
            .and_then(|(_, mut conn)| apply_ai_category_assignments(&mut conn, batch, matched))
        {
            Ok(applied) => applied,
            Err(error) => {
                failed += batch.len();
                emit_ai_failed_progress(
                    &app,
                    task,
                    "拆分分类失败",
                    error.clone(),
                    notes.len(),
                    scanned,
                    updated,
                    failed,
                    scanned.saturating_sub(updated),
                );
                return Err(error);
            }
        };
        let applied_count = applied.len();
        updated += applied_count;
        assignments.extend(applied);
        scanned += batch.len();
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "batch_completed".to_string(),
                label: "拆分当前分类".to_string(),
                detail: format!(
                    "已完成第 {} / {batch_count} 批，命中 {updated} 条。",
                    batch_index + 1
                ),
                planned: notes.len(),
                scanned,
                updated,
                failed,
                skipped: scanned.saturating_sub(updated),
                progress: ai_progress_percent(scanned, notes.len()),
                indeterminate: false,
                error: None,
            },
        );
        if applied_count > 0 {
            emit_library_changed(
                &app,
                format!(
                    "AI 已写入第 {} / {batch_count} 批，命中 {updated} 条。",
                    batch_index + 1
                ),
            );
        }
    }

    let created_categories = if updated > 0 {
        vec![target_category.clone()]
    } else {
        Vec::new()
    };
    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "completed".to_string(),
            label: "拆分分类完成".to_string(),
            detail: format!(
                "AI 已从「{source_category}」筛出 {updated} 条到「{target_category}」。"
            ),
            planned: notes.len(),
            scanned: notes.len(),
            updated,
            failed,
            skipped: notes.len().saturating_sub(updated),
            progress: 100,
            indeterminate: false,
            error: None,
        },
    );
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
    reset_ai_task_cancel();
    let prompt_catalog = active_ai_prompt_catalog(&app)?;
    let config = {
        let (paths, conn) = open_library(&app)?;
        read_ai_runtime_config(&conn, &paths)?
    };
    let limit = input
        .limit
        .unwrap_or(AI_TAG_GROUP_DEFAULT_LIMIT)
        .clamp(1, 600);
    let (categories, tags, category_hints) = {
        let (_, conn) = open_library(&app)?;
        (
            read_category_names(&conn).map_err(|error| error.to_string())?,
            read_ungrouped_tag_summaries_limited(&conn, limit)
                .map_err(|error| error.to_string())?,
            read_tag_category_hints_limited(&conn, limit).map_err(|error| error.to_string())?,
        )
    };
    let task = "group_tags";
    if tags.is_empty() {
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "completed".to_string(),
                label: "整理标签完成".to_string(),
                detail: "没有未归属标签需要整理。".to_string(),
                planned: 0,
                scanned: 0,
                updated: 0,
                failed: 0,
                skipped: 0,
                progress: 100,
                indeterminate: false,
                error: None,
            },
        );
        return Ok(AiTagGroupResult {
            scanned: 0,
            updated: 0,
            groups: Vec::new(),
            assignments: Vec::new(),
            message: "没有未归属标签需要整理。".to_string(),
        });
    }
    if categories.is_empty() {
        emit_ai_progress(
            &app,
            AiJobProgress {
                task: task.to_string(),
                phase: "completed".to_string(),
                label: "整理标签完成".to_string(),
                detail: "还没有可用分类，先完成收藏大分类后再整理标签。".to_string(),
                planned: tags.len(),
                scanned: tags.len(),
                updated: 0,
                failed: 0,
                skipped: tags.len(),
                progress: 100,
                indeterminate: false,
                error: None,
            },
        );
        return Ok(AiTagGroupResult {
            scanned: tags.len(),
            updated: 0,
            groups: Vec::new(),
            assignments: Vec::new(),
            message: "还没有可用分类，先完成收藏大分类后再整理标签。".to_string(),
        });
    }

    let (mut assignments, ai_target_tags) =
        auto_assign_tags_to_categories(&tags, &category_hints, &categories);

    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: if ai_target_tags.is_empty() {
                "applying".to_string()
            } else {
                "requesting".to_string()
            },
            label: "按分类整理标签".to_string(),
            detail: format!(
                "已按笔记分类自动归属 {} 个标签，{} 个标签交给 AI 判断。",
                assignments.len(),
                ai_target_tags.len()
            ),
            planned: tags.len(),
            scanned: tags.len().saturating_sub(ai_target_tags.len()),
            updated: assignments.len(),
            failed: 0,
            skipped: 0,
            progress: if ai_target_tags.is_empty() { 72 } else { 35 },
            indeterminate: false,
            error: None,
        },
    );
    if ai_task_cancel_requested() {
        return Err(emit_ai_cancelled_progress(
            &app,
            task,
            "整理标签已停止",
            tags.len(),
            0,
            0,
            0,
            0,
        ));
    }
    if !ai_target_tags.is_empty() {
        let ai_assignments = match group_tags_with_ai(
            &config,
            &prompt_catalog.prompts.group_tags,
            &categories,
            &ai_target_tags,
            &category_hints,
        )
        .await
        {
            Ok(assignments) => assignments,
            Err(error) => {
                if ai_task_cancel_requested() {
                    return Err(emit_ai_cancelled_progress(
                        &app,
                        task,
                        "整理标签已停止",
                        tags.len(),
                        tags.len().saturating_sub(ai_target_tags.len()),
                        assignments.len(),
                        0,
                        0,
                    ));
                }
                emit_ai_failed_progress(
                    &app,
                    task,
                    "整理标签失败",
                    error.clone(),
                    tags.len(),
                    tags.len().saturating_sub(ai_target_tags.len()),
                    assignments.len(),
                    tags.len(),
                    0,
                );
                return Err(error);
            }
        };
        merge_tag_group_assignments(&mut assignments, ai_assignments);
    }
    if ai_task_cancel_requested() {
        return Err(emit_ai_cancelled_progress(
            &app,
            task,
            "整理标签已停止",
            tags.len(),
            tags.len(),
            assignments.len(),
            0,
            tags.len().saturating_sub(assignments.len()),
        ));
    }
    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "applying".to_string(),
            label: "按分类整理标签".to_string(),
            detail: format!("正在增量写入 {} 条标签分类归属。", assignments.len()),
            planned: tags.len(),
            scanned: tags.len(),
            updated: 0,
            failed: 0,
            skipped: tags.len().saturating_sub(assignments.len()),
            progress: 88,
            indeterminate: false,
            error: None,
        },
    );
    if ai_task_cancel_requested() {
        return Err(emit_ai_cancelled_progress(
            &app,
            task,
            "整理标签已停止",
            tags.len(),
            tags.len(),
            0,
            0,
            tags.len().saturating_sub(assignments.len()),
        ));
    }
    let updated = match open_library(&app).and_then(|(_, mut conn)| {
        apply_ai_tag_groups(&mut conn, assignments.clone(), &categories, false)
    }) {
        Ok(updated) => updated,
        Err(error) => {
            emit_ai_failed_progress(
                &app,
                task,
                "整理标签失败",
                error.clone(),
                tags.len(),
                tags.len(),
                0,
                tags.len(),
                tags.len().saturating_sub(assignments.len()),
            );
            return Err(error);
        }
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

    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "completed".to_string(),
            label: "按分类整理标签完成".to_string(),
            detail: format!("已按当前大分类整理 {updated} 个标签。"),
            planned: tags.len(),
            scanned: tags.len(),
            updated,
            failed: 0,
            skipped: tags.len().saturating_sub(updated),
            progress: 100,
            indeterminate: false,
            error: None,
        },
    );

    Ok(AiTagGroupResult {
        scanned: tags.len(),
        updated,
        groups,
        assignments,
        message: format!("已按当前大分类整理 {updated} 个标签。"),
    })
}

#[tauri::command]
pub(crate) async fn ai_suggest_tag_merges(
    app: AppHandle,
    input: AiTagMergeSuggestInput,
) -> Result<TagGovernanceSuggestionResult, String> {
    reset_ai_task_cancel();
    let limit = input
        .limit
        .unwrap_or(AI_TAG_GROUP_DEFAULT_LIMIT)
        .clamp(1, 800);
    let min_confidence = input.min_confidence.unwrap_or(0.72).clamp(0.0, 1.0);
    let use_ai = input.use_ai.unwrap_or(false);
    let tags = {
        let (_, conn) = open_library(&app)?;
        read_tag_summaries_limited(&conn, limit).map_err(|error| error.to_string())?
    };
    let task = "tag_governance";
    let cleanup_issues = detect_tag_cleanup_issues(&tags);
    let mut merge_groups = if use_ai {
        detect_rule_tag_merges(&tags)
    } else {
        Vec::new()
    };

    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: if use_ai {
                "requesting".to_string()
            } else {
                "completed".to_string()
            },
            label: if use_ai {
                "AI 标签去重".to_string()
            } else {
                "扫描标签问题".to_string()
            },
            detail: if use_ai {
                format!(
                    "已完成规则扫描，正在让 AI 判断 {} 个标签的语义重复。",
                    tags.len()
                )
            } else {
                format!("扫描完成：发现 {} 个待清理标签。", cleanup_issues.len())
            },
            planned: tags.len(),
            scanned: tags.len(),
            updated: merge_groups.len(),
            failed: 0,
            skipped: cleanup_issues.len(),
            progress: if use_ai { 35 } else { 100 },
            indeterminate: false,
            error: None,
        },
    );

    if use_ai && !tags.is_empty() {
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "AI 标签去重已停止",
                tags.len(),
                tags.len(),
                merge_groups.len(),
                0,
                cleanup_issues.len(),
            ));
        }
        let prompt_catalog = active_ai_prompt_catalog(&app)?;
        let config = {
            let (paths, conn) = open_library(&app)?;
            read_ai_runtime_config(&conn, &paths)?
        };
        let ai_groups = match suggest_tag_merges_with_ai(
            &config,
            &prompt_catalog.prompts.tag_merge_suggestions,
            &tags,
            min_confidence,
        )
        .await
        {
            Ok(groups) => groups,
            Err(error) => {
                if ai_task_cancel_requested() {
                    return Err(emit_ai_cancelled_progress(
                        &app,
                        task,
                        "AI 标签去重已停止",
                        tags.len(),
                        tags.len(),
                        merge_groups.len(),
                        0,
                        cleanup_issues.len(),
                    ));
                }
                emit_ai_failed_progress(
                    &app,
                    task,
                    "AI 标签去重失败",
                    error.clone(),
                    tags.len(),
                    tags.len(),
                    merge_groups.len(),
                    tags.len(),
                    cleanup_issues.len(),
                );
                return Err(error);
            }
        };
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "AI 标签去重已停止",
                tags.len(),
                tags.len(),
                merge_groups.len(),
                0,
                cleanup_issues.len(),
            ));
        }
        merge_tag_suggestions(&mut merge_groups, ai_groups);
    }

    emit_ai_progress(
        &app,
        AiJobProgress {
            task: task.to_string(),
            phase: "completed".to_string(),
            label: "标签治理扫描完成".to_string(),
            detail: format!("发现 {} 个待清理标签。", cleanup_issues.len()),
            planned: tags.len(),
            scanned: tags.len(),
            updated: merge_groups.len(),
            failed: 0,
            skipped: cleanup_issues.len(),
            progress: 100,
            indeterminate: false,
            error: None,
        },
    );

    Ok(TagGovernanceSuggestionResult {
        scanned: tags.len(),
        cleanup_issues,
        merge_groups,
        message: format!("已扫描 {} 个标签。", tags.len()),
    })
}

#[tauri::command]
pub(crate) fn apply_tag_governance(
    app: AppHandle,
    input: TagGovernanceApplyInput,
) -> Result<TagGovernanceApplyResult, String> {
    reset_ai_task_cancel();
    let task = "tag_governance";
    let planned = input.remove_tags.len() + input.merge_groups.len();
    let mut scanned = 0usize;
    let (_, mut conn) = open_library(&app)?;
    let transaction = conn
        .transaction()
        .map_err(|error| format!("启动标签治理事务失败：{error}"))?;
    let mut removed_tags = 0usize;
    let mut merged_tags = 0usize;
    let mut aliases_created = 0usize;
    let mut affected_notes = 0usize;

    for tag in input.remove_tags {
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "标签治理已停止",
                planned,
                scanned,
                removed_tags + merged_tags,
                0,
                0,
            ));
        }
        scanned += 1;
        let tag = clean_tag_label(&tag).unwrap_or_default();
        if tag.is_empty() {
            continue;
        }
        if let Some(tag_id) =
            find_tag_id_by_name(&transaction, &tag).map_err(|error| error.to_string())?
        {
            let count = count_note_tags_for_tag(&transaction, &tag_id)
                .map_err(|error| error.to_string())?;
            transaction
                .execute("DELETE FROM note_tags WHERE tag_id = ?1", params![tag_id])
                .map_err(|error| error.to_string())?;
            transaction
                .execute("DELETE FROM tags WHERE id = ?1", params![tag_id])
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "DELETE FROM tag_aliases WHERE normalized_alias = ?1 OR canonical_tag_id = ?2",
                    params![normalize_tag_alias_key(&tag), tag_id],
                )
                .map_err(|error| error.to_string())?;
            removed_tags += 1;
            affected_notes += usize::try_from(count.max(0)).unwrap_or(0);
        }
    }

    for group in input.merge_groups {
        if ai_task_cancel_requested() {
            return Err(emit_ai_cancelled_progress(
                &app,
                task,
                "标签治理已停止",
                planned,
                scanned,
                removed_tags + merged_tags,
                0,
                0,
            ));
        }
        scanned += 1;
        let Some(canonical_tag) = clean_tag_label(&group.canonical_tag) else {
            continue;
        };
        if canonical_tag.is_empty() || is_noise_tag_name(&canonical_tag) {
            continue;
        }
        let canonical_id = upsert_tag_with_kind(&transaction, &canonical_tag, "topic")
            .map_err(|error| format!("创建主标签失败：{error}"))?;
        let canonical_key = normalize_tag_alias_key(&canonical_tag);
        let source = group.source.as_deref().unwrap_or("manual");
        for duplicate in group.duplicate_tags {
            let Some(duplicate_tag) = clean_tag_label(&duplicate) else {
                continue;
            };
            if duplicate_tag.is_empty() || normalize_tag_alias_key(&duplicate_tag) == canonical_key
            {
                continue;
            }
            let duplicate_key = normalize_tag_alias_key(&duplicate_tag);
            let Some(duplicate_id) = find_tag_id_by_name(&transaction, &duplicate_tag)
                .map_err(|error| error.to_string())?
            else {
                aliases_created += upsert_tag_alias(
                    &transaction,
                    &duplicate_tag,
                    &duplicate_key,
                    &canonical_id,
                    source,
                    group.confidence,
                )
                .map_err(|error| error.to_string())?;
                continue;
            };
            let count = count_note_tags_for_tag(&transaction, &duplicate_id)
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag_id)
                     SELECT note_id, ?1 FROM note_tags WHERE tag_id = ?2",
                    params![canonical_id, duplicate_id],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "DELETE FROM note_tags WHERE tag_id = ?1",
                    params![duplicate_id],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute("DELETE FROM tags WHERE id = ?1", params![duplicate_id])
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "DELETE FROM tag_aliases WHERE canonical_tag_id = ?1",
                    params![duplicate_id],
                )
                .map_err(|error| error.to_string())?;
            aliases_created += upsert_tag_alias(
                &transaction,
                &duplicate_tag,
                &duplicate_key,
                &canonical_id,
                source,
                group.confidence,
            )
            .map_err(|error| error.to_string())?;
            merged_tags += 1;
            affected_notes += usize::try_from(count.max(0)).unwrap_or(0);
        }
    }

    transaction
        .commit()
        .map_err(|error| format!("保存标签治理结果失败：{error}"))?;

    Ok(TagGovernanceApplyResult {
        removed_tags,
        merged_tags,
        aliases_created,
        affected_notes,
        message: format!(
            "标签治理完成：清理 {removed_tags} 个标签，合并 {merged_tags} 个重复标签，写入 {aliases_created} 条别名。"
        ),
    })
}

fn detect_tag_cleanup_issues(tags: &[TagSummary]) -> Vec<TagCleanupIssue> {
    tags.iter()
        .filter(|tag| is_noise_tag_name(&tag.name))
        .map(|tag| TagCleanupIssue {
            tag: tag.name.clone(),
            count: tag.count,
            issue_kind: "timecode".to_string(),
            action: "remove".to_string(),
            confidence: 1.0,
            reason: "看起来像视频时间轴，不适合作为检索标签。".to_string(),
        })
        .collect()
}

fn detect_rule_tag_merges(tags: &[TagSummary]) -> Vec<TagMergeSuggestion> {
    let mut groups: HashMap<String, Vec<&TagSummary>> = HashMap::new();
    for tag in tags {
        if is_noise_tag_name(&tag.name) {
            continue;
        }
        let key = normalize_tag_alias_key(&tag.name);
        if key.is_empty() {
            continue;
        }
        groups.entry(key).or_default().push(tag);
    }

    let mut suggestions = Vec::new();
    for (key, mut variants) in groups {
        if variants.len() < 2 {
            continue;
        }
        variants.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.name.len().cmp(&b.name.len()))
        });
        let canonical_tag = preferred_canonical_tag(&key, &variants);
        let canonical_key = normalize_tag_alias_key(&canonical_tag);
        let duplicate_tags = variants
            .iter()
            .filter(|tag| normalize_tag_alias_key(&tag.name) != canonical_key)
            .map(|tag| tag.name.clone())
            .collect::<Vec<_>>();
        if duplicate_tags.is_empty() {
            continue;
        }
        let affected_notes = variants
            .iter()
            .filter(|tag| normalize_tag_alias_key(&tag.name) != canonical_key)
            .map(|tag| tag.count)
            .sum();
        suggestions.push(TagMergeSuggestion {
            canonical_tag,
            duplicate_tags,
            affected_notes,
            confidence: 0.98,
            reason: "大小写、空格或符号差异，属于同一个标签。".to_string(),
            source: "rule".to_string(),
        });
    }
    merge_tag_suggestions(&mut suggestions, detect_subject_family_tag_merges(tags));
    suggestions.sort_by(|a, b| {
        b.affected_notes
            .cmp(&a.affected_notes)
            .then_with(|| b.confidence.total_cmp(&a.confidence))
    });
    suggestions
}

fn detect_subject_family_tag_merges(tags: &[TagSummary]) -> Vec<TagMergeSuggestion> {
    let mut groups: HashMap<String, Vec<&TagSummary>> = HashMap::new();
    let mut labels: HashMap<String, String> = HashMap::new();
    for tag in tags {
        if is_noise_tag_name(&tag.name) {
            continue;
        }
        let Some(label) = leading_subject_prefix(&tag.name) else {
            continue;
        };
        let key = normalize_tag_alias_key(&label);
        if !is_subject_family_key(&key) {
            continue;
        }
        labels.entry(key.clone()).or_insert(label);
        groups.entry(key).or_default().push(tag);
    }

    let mut suggestions = Vec::new();
    for (key, mut variants) in groups {
        variants.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.name.len().cmp(&b.name.len()))
        });
        let canonical_tag = variants
            .iter()
            .find(|tag| normalize_tag_alias_key(&tag.name) == key)
            .map(|tag| tag.name.clone())
            .or_else(|| labels.get(&key).cloned())
            .unwrap_or_else(|| key.clone());
        let canonical_key = normalize_tag_alias_key(&canonical_tag);
        let duplicate_tags = variants
            .iter()
            .filter(|tag| normalize_tag_alias_key(&tag.name) != canonical_key)
            .map(|tag| tag.name.clone())
            .collect::<Vec<_>>();
        if duplicate_tags.len() < 2 {
            continue;
        }
        let affected_notes = variants
            .iter()
            .filter(|tag| normalize_tag_alias_key(&tag.name) != canonical_key)
            .map(|tag| tag.count)
            .sum();
        suggestions.push(TagMergeSuggestion {
            canonical_tag,
            duplicate_tags,
            affected_notes,
            confidence: 0.86,
            reason: "同一主体的派生标签，可合并成主标签。".to_string(),
            source: "family".to_string(),
        });
    }
    suggestions
}

fn leading_subject_prefix(name: &str) -> Option<String> {
    let cleaned = name.trim().trim_start_matches(&['#', '＃'][..]).trim();
    let mut prefix = String::new();
    let mut has_ascii_letter = false;
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_alphabetic() {
                has_ascii_letter = true;
            }
            prefix.push(ch);
        } else if matches!(
            ch,
            ' ' | '\t' | '_' | '-' | '－' | '—' | '/' | '\\' | '·' | '.'
        ) {
            if prefix.is_empty() {
                continue;
            }
            break;
        } else {
            break;
        }
    }
    let normalized = normalize_tag_alias_key(&prefix);
    if has_ascii_letter && is_subject_family_key(&normalized) {
        Some(prefix)
    } else {
        None
    }
}

fn is_subject_family_key(key: &str) -> bool {
    key.chars().count() >= 4 && key.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn preferred_canonical_tag(key: &str, variants: &[&TagSummary]) -> String {
    match key {
        "ai" => "AI".to_string(),
        "llm" => "LLM".to_string(),
        "ui" => "UI".to_string(),
        "ux" => "UX".to_string(),
        "sci" => "SCI".to_string(),
        "gpt" => "GPT".to_string(),
        "rag" => "RAG".to_string(),
        "pdf" => "PDF".to_string(),
        "prd" => "PRD".to_string(),
        "github" => "GitHub".to_string(),
        "claude" => "Claude".to_string(),
        "claudecode" => "Claude Code".to_string(),
        "codex" => "Codex".to_string(),
        "vibecoding" => "Vibe Coding".to_string(),
        _ => variants
            .first()
            .map(|tag| tag.name.clone())
            .unwrap_or_else(|| key.to_string()),
    }
}

async fn suggest_tag_merges_with_ai(
    config: &AiRuntimeConfig,
    prompt: &AiTagMergePrompt,
    tags: &[TagSummary],
    min_confidence: f64,
) -> Result<Vec<TagMergeSuggestion>, String> {
    let user = json!({
        "task": prompt.task.as_str(),
        "minConfidence": min_confidence,
        "rules": &prompt.rules,
        "tags": tags.iter().filter(|tag| !is_noise_tag_name(&tag.name)).map(|tag| json!({
            "name": tag.name,
            "count": tag.count,
            "kind": tag.kind,
            "groupName": tag.group_name,
        })).collect::<Vec<_>>(),
        "returnJsonShape": prompt.return_json_shape.clone()
    })
    .to_string();
    let value = call_ai_json(config, &prompt.system, &user).await?;
    parse_ai_tag_merge_suggestions(&value, tags, min_confidence)
}

fn parse_ai_tag_merge_suggestions(
    value: &Value,
    tags: &[TagSummary],
    min_confidence: f64,
) -> Result<Vec<TagMergeSuggestion>, String> {
    let items = value
        .get("mergeGroups")
        .or_else(|| value.get("merge_groups"))
        .and_then(Value::as_array)
        .ok_or_else(|| "AI 返回缺少 mergeGroups。".to_string())?;
    let tag_counts = tags
        .iter()
        .map(|tag| (tag.name.clone(), tag.count))
        .collect::<HashMap<_, _>>();
    let normalized_to_name = tags
        .iter()
        .map(|tag| (normalize_tag_alias_key(&tag.name), tag.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut suggestions = Vec::new();
    for item in items {
        let Some(canonical_tag) = item
            .get("canonicalTag")
            .or_else(|| item.get("canonical_tag"))
            .and_then(Value::as_str)
            .and_then(clean_tag_label)
        else {
            continue;
        };
        if canonical_tag.is_empty() || is_noise_tag_name(&canonical_tag) {
            continue;
        }
        let confidence = item
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        if confidence < min_confidence {
            continue;
        }
        let Some(duplicates_value) = item
            .get("duplicateTags")
            .or_else(|| item.get("duplicate_tags"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let canonical_key = normalize_tag_alias_key(&canonical_tag);
        let mut duplicate_tags = Vec::new();
        let mut seen = HashSet::new();
        for raw in duplicates_value {
            let Some(candidate) = raw.as_str().and_then(clean_tag_label) else {
                continue;
            };
            let candidate_key = normalize_tag_alias_key(&candidate);
            if candidate_key.is_empty() || candidate_key == canonical_key {
                continue;
            }
            let Some(existing_name) = normalized_to_name.get(&candidate_key) else {
                continue;
            };
            if seen.insert(candidate_key) {
                duplicate_tags.push(existing_name.clone());
            }
        }
        let canonical_exists = normalized_to_name.contains_key(&canonical_key);
        if duplicate_tags.len() + usize::from(canonical_exists) < 2 {
            continue;
        }
        let affected_notes = duplicate_tags
            .iter()
            .filter_map(|tag| tag_counts.get(tag))
            .copied()
            .sum();
        suggestions.push(TagMergeSuggestion {
            canonical_tag,
            duplicate_tags,
            affected_notes,
            confidence,
            reason: item
                .get("reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .unwrap_or("AI 判断这些标签语义重复。")
                .chars()
                .take(160)
                .collect(),
            source: "ai".to_string(),
        });
    }
    suggestions.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| b.affected_notes.cmp(&a.affected_notes))
    });
    Ok(suggestions)
}

fn merge_tag_suggestions(
    existing: &mut Vec<TagMergeSuggestion>,
    incoming: Vec<TagMergeSuggestion>,
) {
    let mut seen = existing
        .iter()
        .map(tag_merge_signature)
        .collect::<HashSet<_>>();
    for suggestion in incoming {
        let signature = tag_merge_signature(&suggestion);
        if seen.insert(signature) {
            existing.push(suggestion);
        }
    }
    existing.sort_by(|a, b| {
        let source_rank = |source: &str| match source {
            "rule" => 0,
            "family" => 1,
            _ => 2,
        };
        source_rank(&a.source)
            .cmp(&source_rank(&b.source))
            .then_with(|| b.confidence.total_cmp(&a.confidence))
            .then_with(|| b.affected_notes.cmp(&a.affected_notes))
    });
}

fn tag_merge_signature(group: &TagMergeSuggestion) -> String {
    let mut keys = group
        .duplicate_tags
        .iter()
        .map(|tag| normalize_tag_alias_key(tag))
        .collect::<Vec<_>>();
    keys.sort();
    format!(
        "{}|{}",
        normalize_tag_alias_key(&group.canonical_tag),
        keys.join(",")
    )
}

fn clean_tag_label(raw: &str) -> Option<String> {
    let tag = raw
        .trim()
        .trim_start_matches(&['#', '＃'][..])
        .trim()
        .chars()
        .take(64)
        .collect::<String>();
    (!tag.is_empty()).then_some(tag)
}

fn find_tag_id_by_name(conn: &Connection, name: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT id FROM tags WHERE name = ?1 LIMIT 1",
        params![name],
        |row| row.get(0),
    )
    .optional()
}

fn count_note_tags_for_tag(conn: &Connection, tag_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM note_tags WHERE tag_id = ?1",
        params![tag_id],
        |row| row.get(0),
    )
}

fn upsert_tag_alias(
    conn: &Connection,
    alias_name: &str,
    normalized_alias: &str,
    canonical_tag_id: &str,
    source: &str,
    confidence: Option<f64>,
) -> rusqlite::Result<usize> {
    if normalized_alias.is_empty() {
        return Ok(0);
    }
    let alias_id = format!("tag-alias:{normalized_alias}");
    conn.execute(
        "INSERT INTO tag_aliases (
            id, alias_name, normalized_alias, canonical_tag_id, source, confidence
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(normalized_alias) DO UPDATE SET
            alias_name = excluded.alias_name,
            canonical_tag_id = excluded.canonical_tag_id,
            source = excluded.source,
            confidence = excluded.confidence,
            updated_at = CURRENT_TIMESTAMP",
        params![
            alias_id,
            alias_name,
            normalized_alias,
            canonical_tag_id,
            source,
            confidence
        ],
    )
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
    extract_json_object(&text).ok_or_else(|| {
        format!(
            "AI 返回内容不是 JSON：{}",
            compact_response_snippet(&text, 180)
        )
    })
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
    let body = openai_compatible_body(config, system, user, true);
    let (mut status, mut text) =
        post_openai_compatible_json(client, &url, config.api_key.trim(), &body).await?;
    if !status.is_success() && should_retry_basic_openai_request(&text) {
        log::warn!(
            "ai_openai_compatible_feature_retry status={} detail={}",
            status.as_u16(),
            compact_response_snippet(&text, 220)
        );
        let fallback_body = openai_compatible_body(config, system, user, false);
        (status, text) =
            post_openai_compatible_json(client, &url, config.api_key.trim(), &fallback_body)
                .await?;
    }
    if !status.is_success() {
        return Err(format!(
            "AI 请求返回 HTTP {}：{}",
            status.as_u16(),
            compact_response_snippet(&text, 260)
        ));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        format!(
            "AI 响应 JSON 解析失败：{error}；响应片段：{}",
            compact_response_snippet(&text, 260)
        )
    })?;
    if let Some(error) = provider_error_message(&value) {
        return Err(format!("AI 请求返回错误：{error}"));
    }
    if let Some(content) = extract_openai_compatible_content(&value) {
        return Ok(content);
    }
    let reasoning_only = value
        .pointer("/choices/0/message/reasoning")
        .or_else(|| value.pointer("/choices/0/message/reasoning_content"))
        .is_some();
    let finish_reason = value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let reason_hint = if reasoning_only {
        "AI 响应只有 reasoning，缺少最终正文 content"
    } else {
        "AI 响应缺少 message.content"
    };
    Err(format!(
        "{reason_hint}，finish_reason={finish_reason}：{}",
        compact_response_snippet(&text, 260)
    ))
}

fn openai_compatible_body(
    config: &AiRuntimeConfig,
    system: &str,
    user: &str,
    structured: bool,
) -> Value {
    let mut body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "temperature": config.temperature
    });
    if let Some(object) = body.as_object_mut() {
        if should_use_max_completion_tokens(config) {
            object.insert(
                "max_completion_tokens".to_string(),
                json!(config.max_tokens.max(1024)),
            );
        } else {
            object.insert("max_tokens".to_string(), json!(config.max_tokens.max(1024)));
        }
        if structured {
            object.insert(
                "response_format".to_string(),
                json!({
                    "type": "json_object"
                }),
            );
            if should_minimize_reasoning(config) {
                object.insert("reasoning_effort".to_string(), json!("minimal"));
                object.insert(
                    "reasoning".to_string(),
                    json!({
                        "exclude": true
                    }),
                );
                object.insert("verbosity".to_string(), json!("low"));
            }
        }
    }
    body
}

async fn post_openai_compatible_json(
    client: &Client,
    url: &str,
    api_key: &str,
    body: &Value,
) -> Result<(StatusCode, String), String> {
    let response = client
        .post(url)
        .bearer_auth(api_key)
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
    Ok((status, text))
}

fn should_use_max_completion_tokens(config: &AiRuntimeConfig) -> bool {
    let base_url = config.base_url.to_lowercase();
    let model = config.model.to_lowercase();
    base_url.contains("openrouter.ai")
        || base_url.contains("api.openai.com")
        || is_openai_reasoning_model(&model)
}

fn should_minimize_reasoning(config: &AiRuntimeConfig) -> bool {
    let base_url = config.base_url.to_lowercase();
    let model = config.model.to_lowercase();
    base_url.contains("openrouter.ai")
        || base_url.contains("api.openai.com")
        || is_openai_reasoning_model(&model)
}

fn is_openai_reasoning_model(model: &str) -> bool {
    let model = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .trim_start_matches("openai:");
    model.contains("gpt-5")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
}

fn should_retry_basic_openai_request(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("unsupported")
        || lower.contains("unrecognized")
        || lower.contains("unknown parameter")
        || lower.contains("invalid parameter")
        || lower.contains("response_format")
        || lower.contains("max_completion_tokens")
        || lower.contains("reasoning_effort")
        || lower.contains("\"reasoning\"")
        || lower.contains("verbosity")
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
            compact_response_snippet(&text, 260)
        ));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        format!(
            "Claude 响应 JSON 解析失败：{error}；响应片段：{}",
            compact_response_snippet(&text, 260)
        )
    })?;
    if let Some(error) = provider_error_message(&value) {
        return Err(format!("Claude 请求返回错误：{error}"));
    }
    extract_text_from_value(value.get("content")).ok_or_else(|| {
        format!(
            "Claude 响应缺少 content.text：{}",
            compact_response_snippet(&text, 260)
        )
    })
}

fn provider_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    if let Some(message) = error.get("message").and_then(Value::as_str) {
        return Some(truncate_chars(message.trim(), 260));
    }
    if let Some(message) = error.as_str() {
        return Some(truncate_chars(message.trim(), 260));
    }
    Some(compact_response_snippet(&error.to_string(), 260))
}

fn extract_openai_compatible_content(value: &Value) -> Option<String> {
    extract_text_from_value(value.pointer("/choices/0/message/content"))
        .or_else(|| extract_text_from_value(value.pointer("/choices/0/text")))
        .or_else(|| extract_text_from_value(value.get("output_text")))
        .or_else(|| extract_text_from_value(value.get("output")))
}

fn extract_text_from_value(value: Option<&Value>) -> Option<String> {
    let mut parts = Vec::new();
    collect_text_parts(value?, &mut parts);
    let text = parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn collect_text_parts(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => output.push(text.clone()),
        Value::Array(items) => {
            for item in items {
                collect_text_parts(item, output);
            }
        }
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                output.push(text.to_string());
            }
            if let Some(content) = object.get("content") {
                collect_text_parts(content, output);
            }
        }
        _ => {}
    }
}

fn ai_endpoint_url(base_url: &str, suffix: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with(suffix) {
        base.to_string()
    } else {
        format!("{base}/{suffix}")
    }
}

fn ai_progress_percent(scanned: usize, planned: usize) -> u8 {
    if planned == 0 {
        return 100;
    }
    (((scanned * 100) / planned).min(100)) as u8
}

#[allow(clippy::too_many_arguments)]
fn emit_ai_failed_progress(
    app: &AppHandle,
    task: &str,
    label: &str,
    error: String,
    planned: usize,
    scanned: usize,
    updated: usize,
    failed: usize,
    skipped: usize,
) {
    let normalized_error = normalize_ai_error(&error);
    let (kind, _) = classify_ai_error(&error);
    log::warn!(
        "ai_progress_failed task={} kind={} progress={} planned={} scanned={} updated={} failed={} skipped={} detail={}",
        task,
        kind,
        ai_progress_percent(scanned, planned),
        planned,
        scanned,
        updated,
        failed,
        skipped,
        truncate_chars(&normalized_error, 360)
    );
    emit_ai_progress(
        app,
        AiJobProgress {
            task: task.to_string(),
            phase: "failed".to_string(),
            label: label.to_string(),
            detail: normalized_error.clone(),
            planned,
            scanned,
            updated,
            failed,
            skipped,
            progress: ai_progress_percent(scanned, planned),
            indeterminate: false,
            error: Some(normalized_error),
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_ai_cancelled_progress(
    app: &AppHandle,
    task: &str,
    label: &str,
    planned: usize,
    scanned: usize,
    updated: usize,
    failed: usize,
    skipped: usize,
) -> String {
    log::info!(
        "ai_progress_cancelled task={} progress={} planned={} scanned={} updated={} failed={} skipped={}",
        task,
        ai_progress_percent(scanned, planned),
        planned,
        scanned,
        updated,
        failed,
        skipped
    );
    emit_ai_progress(
        app,
        AiJobProgress {
            task: task.to_string(),
            phase: "cancelled".to_string(),
            label: label.to_string(),
            detail: AI_CANCELLED_MESSAGE.to_string(),
            planned,
            scanned,
            updated,
            failed,
            skipped,
            progress: ai_progress_percent(scanned, planned),
            indeterminate: false,
            error: None,
        },
    );
    AI_CANCELLED_MESSAGE.to_string()
}

fn emit_ai_progress(app: &AppHandle, progress: AiJobProgress) {
    log::info!(
        "ai_progress task={} phase={} progress={} planned={} scanned={} updated={} failed={} skipped={}",
        progress.task,
        progress.phase,
        progress.progress,
        progress.planned,
        progress.scanned,
        progress.updated,
        progress.failed,
        progress.skipped
    );
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = app.emit_to("main", "ai-job-progress", progress) {
            log::warn!("ai_progress_emit_failed {error}");
        }
    });
}

fn emit_library_changed(app: &AppHandle, message: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = app.emit_to("main", "library-changed", message) {
            log::warn!("library_changed_emit_failed {error}");
        }
    });
}

fn normalize_ai_error(error: &str) -> String {
    let trimmed = error.trim();
    let (kind, suggestion) = classify_ai_error(trimmed);
    let brief = truncate_chars(trimmed, 220);
    format!("{kind}：{brief}。{suggestion}")
}

fn classify_ai_error(error: &str) -> (&'static str, &'static str) {
    let lower = error.to_lowercase();
    if lower.contains("only one of")
        || lower.contains("unknown parameter")
        || lower.contains("invalid parameter")
        || lower.contains("unsupported parameter")
        || lower.contains("reasoning.effort")
        || lower.contains("reasoning.max_tokens")
        || lower.contains("response_format")
        || lower.contains("max_completion_tokens")
        || lower.contains("reasoning_effort")
        || lower.contains("verbosity")
    {
        return (
            "AI 模型或端点不匹配",
            "请确认模型名、Base URL 和服务商配置一致；如果是兼容端点参数差异，应用会尝试降级请求。",
        );
    }
    if lower.contains("不是 json")
        || lower.contains("json")
        || lower.contains("缺少 assignments")
        || lower.contains("缺少 matches")
        || lower.contains("message.content")
        || lower.contains("content.text")
        || lower.contains("缺少最终正文")
        || lower.contains("缺少 content")
        || lower.contains("finish_reason")
        || lower.contains("empty response")
    {
        return (
            "AI 返回格式异常",
            "模型没有按要求返回结构化 JSON，建议重试、降低数量，或换更稳定的模型。",
        );
    }
    if lower.contains("api key")
        || lower.contains("unauthorized")
        || lower.contains("invalid_api_key")
        || lower.contains("401")
        || lower.contains("403")
        || lower.contains("认证")
        || lower.contains("keychain")
    {
        return (
            "AI 配置或鉴权失败",
            "请检查 API Key、服务商、Base URL 和模型是否属于同一个平台。",
        );
    }
    if lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("429")
        || lower.contains("限流")
        || lower.contains("频率")
    {
        return (
            "AI 请求被限流",
            "可以稍后重试，或降低本次处理数量、换用更高额度的模型。",
        );
    }
    if lower.contains("quota")
        || lower.contains("insufficient")
        || lower.contains("balance")
        || lower.contains("余额")
        || lower.contains("额度")
    {
        return (
            "AI 额度不足",
            "请检查账户余额、套餐额度或 OpenRouter/SiliconFlow 等聚合平台的模型额度。",
        );
    }
    if lower.contains("model")
        || lower.contains("not found")
        || lower.contains("404")
        || lower.contains("模型")
    {
        return (
            "AI 模型或端点不匹配",
            "请确认模型名、Base URL 和服务商配置一致，例如 OpenAI-compatible 端点需以 /v1 结尾。",
        );
    }
    if lower.contains("context")
        || lower.contains("maximum")
        || lower.contains("token")
        || lower.contains("too long")
        || lower.contains("上下文")
    {
        return (
            "AI 上下文过长",
            "请减少本次处理数量，或换用更长上下文的模型。",
        );
    }
    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("deadline")
        || lower.contains("超时")
    {
        return (
            "AI 请求超时",
            "可以重试，或减少本次处理数量；如果模型响应慢，建议换轻量模型。",
        );
    }
    if lower.contains("dns")
        || lower.contains("connect")
        || lower.contains("connection")
        || lower.contains("network")
        || lower.contains("tls")
        || lower.contains("请求失败")
    {
        return ("AI 网络连接失败", "请检查网络、代理、Base URL 是否可访问。");
    }
    if lower.contains("http 5")
        || lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
    {
        return (
            "AI 服务端暂时不可用",
            "这通常是服务商侧问题，可以稍后重试或切换模型。",
        );
    }
    if lower.contains("数据库")
        || lower.contains("写入")
        || lower.contains("sqlite")
        || lower.contains("sql")
    {
        return ("本地写入失败", "请确认数据库文件没有被占用，稍后重试。");
    }
    (
        "AI 任务异常",
        "请查看日志中的 detail，并尝试缩小本次处理数量后重试。",
    )
}

fn should_retry_smaller_ai_batch(error: &str, batch_len: usize) -> bool {
    if batch_len <= 1 {
        return false;
    }
    let lower = error.to_lowercase();
    lower.contains("finish_reason=length")
        || lower.contains("缺少最终正文")
        || lower.contains("只有 reasoning")
        || lower.contains("context")
        || lower.contains("too long")
        || lower.contains("上下文")
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
    extract_first_balanced_json_object(without_fence)
}

fn extract_first_balanced_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = start + offset + ch.len_utf8();
                    if let Ok(value) = serde_json::from_str::<Value>(&text[start..end]) {
                        return Some(value);
                    }
                }
            }
            _ => {}
        }
    }
    None
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

fn read_ungrouped_tag_summaries_limited(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<TagSummary>> {
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(nt.note_id) AS usage_count, t.kind, t.ai_group
         FROM tags t
         LEFT JOIN note_tags nt ON nt.tag_id = t.id
         WHERE t.ai_group IS NULL OR TRIM(t.ai_group) = ''
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

fn read_tag_category_hints_limited(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<HashMap<String, TagCategoryHint>> {
    let mut stmt = conn.prepare(
        "WITH limited_tags AS (
            SELECT t.id, t.name
            FROM tags t
            LEFT JOIN note_tags nt ON nt.tag_id = t.id
            WHERE t.ai_group IS NULL OR TRIM(t.ai_group) = ''
            GROUP BY t.id, t.name
            ORDER BY COUNT(nt.note_id) DESC, t.name ASC
            LIMIT ?1
         )
         SELECT lt.name, c.name, COUNT(nt.note_id) AS usage_count
         FROM limited_tags lt
         LEFT JOIN note_tags nt ON nt.tag_id = lt.id
         LEFT JOIN notes n ON n.id = nt.note_id
         LEFT JOIN categories c ON c.id = n.category_id
         GROUP BY lt.id, lt.name, c.name
         ORDER BY lt.name ASC, usage_count DESC",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut hints = HashMap::<String, TagCategoryHint>::new();
    for row in rows {
        let (tag_name, category_name, count) = row?;
        if count <= 0 {
            hints.entry(tag_name).or_insert_with(|| TagCategoryHint {
                candidates: Vec::new(),
                categorized_total: 0,
            });
            continue;
        }
        let Some(category_name) = category_name else {
            continue;
        };
        let category_name = category_name.trim();
        if category_name.is_empty() {
            continue;
        }
        let hint = hints.entry(tag_name).or_insert_with(|| TagCategoryHint {
            candidates: Vec::new(),
            categorized_total: 0,
        });
        hint.categorized_total += count;
        hint.candidates.push((category_name.to_string(), count));
    }
    Ok(hints)
}

async fn classify_notes_with_ai(
    config: &AiRuntimeConfig,
    prompt: &AiTaskPrompt,
    categories: &[String],
    notes: &[AiNoteDigest],
) -> Result<Vec<AiAssignmentResult>, String> {
    let user = json!({
        "task": prompt.task.as_str(),
        "existingCategories": categories,
        "notes": notes.iter().map(ai_note_payload).collect::<Vec<_>>(),
        "outputSchema": prompt.output_schema.clone()
    });
    let value = call_ai_json(config, &prompt.system, &user.to_string()).await?;
    parse_ai_category_assignments(&value, notes, None)
}

async fn classify_notes_with_smaller_batches(
    config: &AiRuntimeConfig,
    prompt: &AiTaskPrompt,
    categories: &[String],
    notes: &[AiNoteDigest],
) -> Result<Vec<AiAssignmentResult>, String> {
    let retry_size = AI_CLASSIFY_RETRY_BATCH_SIZE
        .min(notes.len().saturating_sub(1))
        .max(1);
    let mut assignments = Vec::new();
    for chunk in notes.chunks(retry_size) {
        if ai_task_cancel_requested() {
            return Err(AI_CANCELLED_MESSAGE.to_string());
        }
        match classify_notes_with_ai(config, prompt, categories, chunk).await {
            Ok(mut chunk_assignments) => assignments.append(&mut chunk_assignments),
            Err(error) if chunk.len() > 1 => {
                log::warn!(
                    "ai_classify_retry_single chunk_size={} error={}",
                    chunk.len(),
                    truncate_chars(&error, 220)
                );
                for note in chunk.chunks(1) {
                    if ai_task_cancel_requested() {
                        return Err(AI_CANCELLED_MESSAGE.to_string());
                    }
                    let mut single_assignments =
                        classify_notes_with_ai(config, prompt, categories, note).await?;
                    assignments.append(&mut single_assignments);
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(assignments)
}

async fn filter_category_notes_with_ai(
    config: &AiRuntimeConfig,
    prompt: &AiTaskPrompt,
    source_category: &str,
    target_category: &str,
    query: &str,
    notes: &[AiNoteDigest],
) -> Result<Vec<AiAssignmentResult>, String> {
    let user = json!({
        "task": prompt.task.as_str(),
        "sourceCategory": source_category,
        "targetCategory": target_category,
        "userRule": query,
        "notes": notes.iter().map(ai_note_payload).collect::<Vec<_>>(),
        "outputSchema": prompt.output_schema.clone()
    });
    let value = call_ai_json(config, &prompt.system, &user.to_string()).await?;
    parse_ai_filter_assignments(&value, notes, target_category)
}

async fn group_tags_with_ai(
    config: &AiRuntimeConfig,
    prompt: &AiTaskPrompt,
    categories: &[String],
    tags: &[TagSummary],
    category_hints: &HashMap<String, TagCategoryHint>,
) -> Result<Vec<AiTagAssignmentResult>, String> {
    let user = json!({
        "task": prompt.task.as_str(),
        "existingCategories": categories,
        "tags": tags.iter().map(|tag| json!({
            "name": tag.name,
            "count": tag.count,
            "kind": tag.kind,
            "categoryCandidates": category_hint_payload(category_hints.get(&tag.name))
        })).collect::<Vec<_>>(),
        "outputSchema": prompt.output_schema.clone()
    });
    let value = call_ai_json(config, &prompt.system, &user.to_string()).await?;
    let assignments = parse_ai_tag_assignments(&value, tags)?;
    Ok(filter_tag_group_assignments_to_categories(
        assignments,
        categories,
    ))
}

fn auto_assign_tags_to_categories(
    tags: &[TagSummary],
    category_hints: &HashMap<String, TagCategoryHint>,
    categories: &[String],
) -> (Vec<AiTagAssignmentResult>, Vec<TagSummary>) {
    let category_lookup = categories
        .iter()
        .map(|name| (name.to_lowercase(), name.clone()))
        .collect::<HashMap<_, _>>();
    let mut assignments = Vec::new();
    let mut ai_target_tags = Vec::new();
    for tag in tags {
        let Some(hint) = category_hints.get(&tag.name) else {
            ai_target_tags.push(tag.clone());
            continue;
        };
        let candidates = hint
            .candidates
            .iter()
            .filter_map(|(category, count)| {
                category_lookup
                    .get(&category.to_lowercase())
                    .map(|canonical| (canonical.clone(), *count))
            })
            .filter(|(_, count)| *count > 0)
            .collect::<Vec<_>>();
        let categorized_total = hint
            .categorized_total
            .max(candidates.iter().map(|(_, count)| *count).sum::<i64>());
        let Some((category_name, top_count)) = candidates.iter().max_by_key(|(_, count)| *count)
        else {
            ai_target_tags.push(tag.clone());
            continue;
        };
        if categorized_total <= 0 {
            ai_target_tags.push(tag.clone());
            continue;
        }
        let confidence = (*top_count as f64 / categorized_total as f64).clamp(0.0, 1.0);
        if confidence < TAG_CATEGORY_AUTO_CONFIDENCE_THRESHOLD {
            ai_target_tags.push(tag.clone());
            continue;
        }
        assignments.push(AiTagAssignmentResult {
            tag: tag.name.clone(),
            group_name: category_name.clone(),
            confidence: confidence.max(0.65),
            reason: format!(
                "已分类笔记中 {top_count}/{categorized_total} 条属于「{category_name}」。"
            ),
        });
    }
    (assignments, ai_target_tags)
}

fn category_hint_payload(hint: Option<&TagCategoryHint>) -> Value {
    let candidates = hint
        .map(|hint| {
            hint.candidates
                .iter()
                .map(|(category, count)| {
                    json!({
                        "category": category,
                        "count": count
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(candidates)
}

fn filter_tag_group_assignments_to_categories(
    assignments: Vec<AiTagAssignmentResult>,
    categories: &[String],
) -> Vec<AiTagAssignmentResult> {
    let category_lookup = categories
        .iter()
        .map(|name| (name.to_lowercase(), name.clone()))
        .collect::<HashMap<_, _>>();
    assignments
        .into_iter()
        .filter_map(|assignment| {
            if assignment.confidence < AI_MIN_ASSIGNMENT_CONFIDENCE {
                return None;
            }
            let canonical = category_lookup.get(&assignment.group_name.to_lowercase())?;
            Some(AiTagAssignmentResult {
                group_name: canonical.clone(),
                ..assignment
            })
        })
        .collect()
}

fn merge_tag_group_assignments(
    existing: &mut Vec<AiTagAssignmentResult>,
    incoming: Vec<AiTagAssignmentResult>,
) {
    let mut seen = existing
        .iter()
        .map(|assignment| assignment.tag.to_lowercase())
        .collect::<HashSet<_>>();
    for assignment in incoming {
        if seen.insert(assignment.tag.to_lowercase()) {
            existing.push(assignment);
        }
    }
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
        if assignment.confidence < AI_MIN_ASSIGNMENT_CONFIDENCE {
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
    categories: &[String],
    clear_existing: bool,
) -> Result<usize, String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    if clear_existing {
        transaction
            .execute(
                "UPDATE tags
                 SET ai_group = NULL,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE ai_group IS NOT NULL",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    let category_lookup = categories
        .iter()
        .map(|name| (name.to_lowercase(), name.clone()))
        .collect::<HashMap<_, _>>();
    let mut updated = 0;
    for assignment in assignments {
        let Some(group_name) = sanitize_category_name(&assignment.group_name) else {
            continue;
        };
        let Some(group_name) = category_lookup.get(&group_name.to_lowercase()).cloned() else {
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

fn compact_response_snippet(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        "<empty response>".to_string()
    } else {
        truncate_chars(&compact, max_chars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, count: i64) -> TagSummary {
        TagSummary {
            name: name.to_string(),
            count,
            kind: "topic".to_string(),
            group_name: None,
        }
    }

    #[test]
    fn bundled_ai_prompts_load() {
        let catalog =
            parse_ai_prompt_catalog(AI_PROMPTS_YAML).expect("AI prompt YAML should parse");

        assert_eq!(catalog.version, 1);
        assert!(!catalog.prompts.test_connection.system.trim().is_empty());
        assert!(catalog
            .prompts
            .classify_uncategorized
            .output_schema
            .get("assignments")
            .is_some());
        assert!(catalog
            .prompts
            .split_category
            .output_schema
            .get("matches")
            .is_some());
        assert!(catalog
            .prompts
            .group_tags
            .output_schema
            .get("assignments")
            .is_some());
        assert!(catalog
            .prompts
            .tag_merge_suggestions
            .return_json_shape
            .get("mergeGroups")
            .is_some());
    }

    #[test]
    fn tag_merge_detects_subject_family_prefixes() {
        let suggestions = detect_rule_tag_merges(&[
            tag("pokopia建筑", 3),
            tag("pokopia日常", 2),
            tag("pokopia装修", 4),
            tag("Agent", 1),
        ]);
        let family = suggestions
            .iter()
            .find(|group| group.canonical_tag == "pokopia")
            .expect("pokopia subject family should be suggested");

        assert_eq!(family.source, "family");
        assert_eq!(family.affected_notes, 9);
        assert!(family.duplicate_tags.contains(&"pokopia建筑".to_string()));
        assert!(family.duplicate_tags.contains(&"pokopia日常".to_string()));
        assert!(family.duplicate_tags.contains(&"pokopia装修".to_string()));
    }

    #[test]
    fn tag_merge_does_not_collapse_short_generic_prefixes() {
        let suggestions = detect_rule_tag_merges(&[
            tag("AI工具", 3),
            tag("AI编程", 2),
            tag("AI绘画", 4),
            tag("UI设计", 1),
            tag("UI灵感", 1),
        ]);

        assert!(suggestions.is_empty());
    }

    #[test]
    fn openai_content_parts_are_joined() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": [
                        { "type": "text", "text": "{\"ok\":" },
                        { "type": "text", "text": "true}" }
                    ]
                }
            }]
        });

        assert_eq!(
            extract_openai_compatible_content(&response).as_deref(),
            Some("{\"ok\":\ntrue}")
        );
    }

    #[test]
    fn provider_errors_are_extracted() {
        let response = json!({
            "error": {
                "message": "No endpoints found for this model."
            }
        });

        assert_eq!(
            provider_error_message(&response).as_deref(),
            Some("No endpoints found for this model.")
        );
    }

    #[test]
    fn response_snippet_compacts_blank_lines() {
        assert_eq!(
            compact_response_snippet("\n\n   {\"error\": \"bad\"}\n\n", 120),
            "{\"error\": \"bad\"}"
        );
    }

    #[test]
    fn openrouter_gpt5_body_uses_json_mode_and_low_reasoning() {
        let config = AiRuntimeConfig {
            provider: "openai_compatible".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "openai/gpt-5-nano".to_string(),
            api_key: "test".to_string(),
            temperature: 0.2,
            max_tokens: 4096,
        };
        let body = openai_compatible_body(&config, "只输出 JSON", "{}", true);

        assert_eq!(
            body.pointer("/response_format/type")
                .and_then(Value::as_str),
            Some("json_object")
        );
        assert_eq!(
            body.get("max_completion_tokens").and_then(Value::as_i64),
            Some(4096)
        );
        assert_eq!(
            body.get("reasoning_effort").and_then(Value::as_str),
            Some("minimal")
        );
        assert_eq!(
            body.pointer("/reasoning/max_tokens")
                .and_then(Value::as_i64),
            None
        );
        assert_eq!(
            body.pointer("/reasoning/exclude").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(body.get("verbosity").and_then(Value::as_str), Some("low"));
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn reasoning_length_errors_retry_smaller_batches() {
        assert!(should_retry_smaller_ai_batch(
            "AI 响应只有 reasoning，缺少最终正文 content，finish_reason=length",
            8
        ));
        assert!(!should_retry_smaller_ai_batch(
            "AI 响应只有 reasoning，缺少最终正文 content，finish_reason=length",
            1
        ));
    }
}
