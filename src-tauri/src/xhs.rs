use crate::constants::*;
use crate::models::*;
use crate::storage::{
    find_local_profile_by_xhs_id, is_noise_tag_name, open_library, open_profile_registry,
    read_active_local_profile, set_active_local_profile, upsert_tag_with_kind,
};
use crate::utils::display_path;
use crate::xhs_scripts::*;
use chrono::{DateTime, TimeZone, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use keyring_core::Entry as KeyringEntry;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, COOKIE, REFERER, USER_AGENT};
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{
    webview::Cookie, AppHandle, Emitter, Manager, Runtime, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn load_xhs_saved_session(
    app: AppHandle,
) -> Result<Option<XhsSessionTestResult>, String> {
    let (_, conn) = open_library(&app)?;
    let Some(stored) = read_latest_xhs_stored_session(&conn)
        .map_err(|error| format!("读取本地登录态失败：{error}"))?
    else {
        return Ok(None);
    };
    drop(conn);

    let mut result = test_xhs_cookie(&stored.session_cookie).await?;
    merge_stored_account_into_session_result(&mut result, &stored);

    if result.ok {
        ensure_active_profile_for_xhs_session(&app, &result)?;
    }

    let (_, conn) = open_library(&app)?;
    if result.ok {
        save_xhs_session_with_conn(&conn, &stored.session_cookie, &result)
            .map_err(|error| format!("更新本地登录态失败：{error}"))?;
        mark_active_profile_session_status(&app, &stored.source_account_id, "connected");
    } else {
        mark_xhs_session_status(&conn, &stored.source_account_id, "expired")
            .map_err(|error| format!("标记登录态过期失败：{error}"))?;
        mark_active_profile_session_status(&app, &stored.source_account_id, "expired");
        result.message = format!("本地保存的登录态已过期或不可用。{}", result.message);
    }

    Ok(Some(result))
}

#[tauri::command]
pub(crate) async fn enrich_xhs_note_details(
    app: AppHandle,
    input: BatchJobInput,
) -> Result<BatchJobResult, String> {
    reset_xhs_sync_cancel();
    let limit = input.limit.filter(|count| *count > 0);
    let started_at = Instant::now();
    log::info!(
        "xhs_detail_batch_start limit={}",
        limit.map_or_else(|| "all".to_string(), |count| count.to_string())
    );

    let (_, conn) = open_library(&app)?;
    let targets = read_xhs_detail_targets(&conn, limit)
        .map_err(|error| format!("读取待补全笔记失败：{error}"))?;
    drop(conn);

    let planned = targets.len();
    emit_batch_progress(
        &app,
        "xhs-detail-progress",
        BatchJobProgress {
            phase: "preparing".to_string(),
            label: "准备补全详情".to_string(),
            detail: if planned == 0 {
                "本地库里暂时没有需要补全的笔记。".to_string()
            } else {
                format!("本批将补全 {planned} 条笔记详情。")
            },
            planned,
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            progress: if planned == 0 { 100 } else { 3 },
            indeterminate: false,
        },
    );

    if targets.is_empty() {
        return Ok(BatchJobResult {
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            message: "没有需要补全的笔记详情。".to_string(),
        });
    }

    let cookie_header = read_current_or_saved_xhs_cookie(&app)?
        .ok_or_else(|| "没有可用的小红书登录态，无法批量抓取详情。请先登录一次。".to_string())?;
    if cookie_header.trim().is_empty() {
        return Err("没有读到小红书登录态，无法批量抓取详情。".to_string());
    }
    log::info!(
        "xhs_detail_batch_cookie_ready key_count={}",
        cookie_keys(&cookie_header).len()
    );
    match test_xhs_cookie(&cookie_header).await {
        Ok(session) if session.ok => {
            log::info!(
                "xhs_detail_batch_session_ok account_id={:?}",
                session.account_id
            );
        }
        Ok(session) => {
            log::warn!(
                "xhs_detail_batch_session_soft_failed status={} url={} message={}",
                session.status_code,
                session.final_url,
                session.message
            );
        }
        Err(error) => {
            log::warn!("xhs_detail_batch_session_test_error {error}");
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("初始化小红书详情客户端失败：{error}"))?;

    let mut result = BatchJobResult {
        scanned: 0,
        updated: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        message: String::new(),
    };

    for (index, target) in targets.iter().enumerate() {
        ensure_xhs_sync_not_cancelled()?;
        emit_batch_progress(
            &app,
            "xhs-detail-progress",
            BatchJobProgress {
                phase: "fetching_detail".to_string(),
                label: "抓取笔记详情".to_string(),
                detail: format!(
                    "正在读取第 {} / {planned} 条：{}",
                    index + 1,
                    target.source_note_id
                ),
                planned,
                scanned: result.scanned,
                updated: result.updated,
                downloaded: 0,
                failed: result.failed,
                skipped: result.skipped,
                progress: batch_progress_percent(result.scanned, planned).max(4),
                indeterminate: false,
            },
        );

        match fetch_xhs_note_detail_resilient(&app, &client, &cookie_header, target).await {
            Ok(detail) => {
                let (_, conn) = open_library(&app)?;
                upsert_xhs_note_detail(&conn, &target.id, &target.source_note_id, &detail)
                    .map_err(|error| format!("写入笔记详情失败：{error}"))?;
                result.updated += 1;
            }
            Err(XhsDetailFetchError::Gone(status_code)) => {
                let (_, conn) = open_library(&app)?;
                mark_xhs_note_unavailable(&conn, &target.id, &format!("detail_http_{status_code}"))
                    .map_err(|error| format!("标记失效笔记失败：{error}"))?;
                result.failed += 1;
            }
            Err(XhsDetailFetchError::NeedsVerification(message)) => {
                log::warn!(
                    "xhs_detail_verification_required note_id={} message={}",
                    target.source_note_id,
                    message
                );
                result.skipped = planned.saturating_sub(result.scanned);
                result.message = format!(
                    "小红书要求验证码，已暂停详情补全。已处理 {} 条，补全 {} 条，失败 {} 条。请在弹出的 Tauri 小红书窗口完成验证后再继续。",
                    result.scanned, result.updated, result.failed
                );
                emit_batch_progress(
                    &app,
                    "xhs-detail-progress",
                    BatchJobProgress {
                        phase: "verification_required".to_string(),
                        label: "需要小红书验证".to_string(),
                        detail: result.message.clone(),
                        planned,
                        scanned: result.scanned,
                        updated: result.updated,
                        downloaded: 0,
                        failed: result.failed,
                        skipped: result.skipped,
                        progress: batch_progress_percent(result.scanned, planned).max(4),
                        indeterminate: false,
                    },
                );
                return Ok(result);
            }
            Err(XhsDetailFetchError::Other(error)) => {
                log::warn!(
                    "xhs_detail_fetch_failed note_id={} error={}",
                    target.source_note_id,
                    error
                );
                result.failed += 1;
            }
        }

        result.scanned += 1;
        emit_batch_progress(
            &app,
            "xhs-detail-progress",
            BatchJobProgress {
                phase: "fetching_detail".to_string(),
                label: "抓取笔记详情".to_string(),
                detail: format!(
                    "已处理 {} / {planned} 条，补全 {} 条，失败 {} 条。",
                    result.scanned, result.updated, result.failed
                ),
                planned,
                scanned: result.scanned,
                updated: result.updated,
                downloaded: 0,
                failed: result.failed,
                skipped: result.skipped,
                progress: batch_progress_percent(result.scanned, planned),
                indeterminate: false,
            },
        );

        if index + 1 < planned {
            std::thread::sleep(XHS_REQUEST_DELAY);
        }
    }

    result.message = format!(
        "详情补全完成：处理 {} 条，补全 {} 条，失败 {} 条。耗时 {} 秒。",
        result.scanned,
        result.updated,
        result.failed,
        started_at.elapsed().as_secs()
    );
    emit_batch_progress(
        &app,
        "xhs-detail-progress",
        BatchJobProgress {
            phase: "completed".to_string(),
            label: "详情补全完成".to_string(),
            detail: result.message.clone(),
            planned,
            scanned: result.scanned,
            updated: result.updated,
            downloaded: 0,
            failed: result.failed,
            skipped: result.skipped,
            progress: 100,
            indeterminate: false,
        },
    );

    Ok(result)
}

#[tauri::command]
pub(crate) async fn download_media_assets(
    app: AppHandle,
    input: BatchJobInput,
) -> Result<BatchJobResult, String> {
    reset_xhs_sync_cancel();
    let requested_asset_id = input
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let requested_note_id = input
        .note_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let limit = input
        .limit
        .filter(|count| *count > 0)
        .unwrap_or(XHS_DOWNLOAD_DEFAULT_LIMIT);
    let started_at = Instant::now();
    log::info!(
        "media_download_batch_start limit={} asset_id={:?} note_id={:?}",
        limit,
        requested_asset_id,
        requested_note_id
    );

    let mut cookie_header: Option<String> = None;
    let mut detail_result: Option<BatchJobResult> = None;
    if let Some(note_id) = requested_note_id.as_deref() {
        let cookie = read_current_or_saved_xhs_cookie(&app)?.unwrap_or_default();
        if !cookie.trim().is_empty() {
            detail_result =
                Some(enrich_single_xhs_note_before_media_download(&app, &cookie, note_id).await?);
        } else {
            log::warn!("media_download_note_detail_skipped_no_cookie note_id={note_id}");
        }
        cookie_header = Some(cookie);
    }

    let (paths, conn) = open_library(&app)?;
    let targets = if let Some(asset_id) = requested_asset_id.as_deref() {
        read_media_download_target_by_id(&conn, asset_id)
            .map(|target| target.into_iter().collect::<Vec<_>>())
            .map_err(|error| format!("读取指定媒体失败：{error}"))?
    } else if let Some(note_id) = requested_note_id.as_deref() {
        read_media_download_targets_by_note_id(&conn, note_id, limit)
            .map_err(|error| format!("读取当前笔记媒体失败：{error}"))?
    } else {
        read_media_download_targets(&conn, limit)
            .map_err(|error| format!("读取待下载媒体失败：{error}"))?
    };
    drop(conn);

    let planned = targets.len();
    emit_batch_progress(
        &app,
        "media-download-progress",
        BatchJobProgress {
            phase: "preparing".to_string(),
            label: "准备下载媒体".to_string(),
            detail: if planned == 0 {
                if requested_asset_id.is_some() {
                    "这个媒体资产已经下载，或缺少可下载地址。".to_string()
                } else if requested_note_id.is_some() {
                    "当前笔记的媒体已经下载，或缺少可下载地址。".to_string()
                } else {
                    "本地库里暂时没有可下载的媒体资产。".to_string()
                }
            } else {
                match requested_asset_id.as_ref() {
                    Some(_) => "正在下载当前选中的媒体资产。".to_string(),
                    None if requested_note_id.is_some() => {
                        format!("正在下载当前笔记的 {planned} 个媒体资产。")
                    }
                    None => format!("本批最多下载 {planned} 个媒体资产。"),
                }
            },
            planned,
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            progress: if planned == 0 { 100 } else { 3 },
            indeterminate: false,
        },
    );

    if targets.is_empty() {
        let message = if requested_note_id.is_some()
            && detail_result
                .as_ref()
                .is_some_and(|result| result.failed > 0)
        {
            "当前笔记详情补全失败，暂时没有新的媒体资产可下载。请确认登录态可用后重试。".to_string()
        } else if requested_note_id.is_some()
            && detail_result
                .as_ref()
                .is_some_and(|result| result.updated > 0)
        {
            "当前笔记详情已补全，媒体资产都已下载或没有新的可下载地址。".to_string()
        } else if requested_asset_id.is_some() {
            "这个媒体资产无需下载。".to_string()
        } else if requested_note_id.is_some() {
            "当前笔记没有需要下载的媒体资产。".to_string()
        } else {
            "没有需要下载的媒体资产。".to_string()
        };
        return Ok(BatchJobResult {
            scanned: 0,
            updated: detail_result.as_ref().map_or(0, |result| result.updated),
            downloaded: 0,
            failed: detail_result.as_ref().map_or(0, |result| result.failed),
            skipped: 0,
            message,
        });
    }

    let cookie_header = match cookie_header {
        Some(cookie) => cookie,
        None => read_current_or_saved_xhs_cookie(&app)?.unwrap_or_default(),
    };
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("初始化媒体下载客户端失败：{error}"))?;

    let mut result = BatchJobResult {
        scanned: 0,
        updated: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        message: String::new(),
    };

    let concurrency = media_download_concurrency(planned);
    let mut next_target = targets.into_iter().enumerate();
    let mut active = FuturesUnordered::new();
    for _ in 0..concurrency {
        if let Some((index, target)) = next_target.next() {
            ensure_xhs_sync_not_cancelled()?;
            active.push(download_one_media_target(
                app.clone(),
                paths.media_dir.clone(),
                client.clone(),
                cookie_header.clone(),
                index,
                target,
            ));
        }
    }

    emit_batch_progress(
        &app,
        "media-download-progress",
        BatchJobProgress {
            phase: "downloading".to_string(),
            label: "下载媒体".to_string(),
            detail: format!("正在并行下载 {planned} 个媒体资产，最多 {concurrency} 个同时进行。"),
            planned,
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            progress: 4,
            indeterminate: false,
        },
    );

    while let Some(outcome) = active.next().await {
        if outcome.downloaded {
            result.downloaded += 1;
        } else {
            result.failed += 1;
        }
        result.scanned += 1;
        emit_batch_progress(
            &app,
            "media-download-progress",
            BatchJobProgress {
                phase: "downloading".to_string(),
                label: "下载媒体".to_string(),
                detail: format!(
                    "已处理 {} / {planned} 个，下载 {} 个，失败 {} 个。{}",
                    result.scanned, result.downloaded, result.failed, outcome.detail
                ),
                planned,
                scanned: result.scanned,
                updated: 0,
                downloaded: result.downloaded,
                failed: result.failed,
                skipped: result.skipped,
                progress: batch_progress_percent(result.scanned, planned),
                indeterminate: false,
            },
        );

        if let Some((index, target)) = next_target.next() {
            ensure_xhs_sync_not_cancelled()?;
            active.push(download_one_media_target(
                app.clone(),
                paths.media_dir.clone(),
                client.clone(),
                cookie_header.clone(),
                index,
                target,
            ));
        }
    }

    result.message = format!(
        "媒体下载完成：处理 {} 个，下载 {} 个，失败 {} 个。耗时 {} 秒。",
        result.scanned,
        result.downloaded,
        result.failed,
        started_at.elapsed().as_secs()
    );
    if let Some(detail_result) = detail_result {
        result.updated += detail_result.updated;
        result.failed += detail_result.failed;
        let detail_note = if detail_result.updated > 0 {
            format!(
                "已先补全当前笔记详情，发现新的图片/视频资产。{}",
                result.message
            )
        } else if detail_result.failed > 0 {
            format!("当前笔记详情补全失败，已下载现有媒体。{}", result.message)
        } else {
            result.message.clone()
        };
        result.message = detail_note;
    }
    emit_batch_progress(
        &app,
        "media-download-progress",
        BatchJobProgress {
            phase: "completed".to_string(),
            label: "媒体下载完成".to_string(),
            detail: result.message.clone(),
            planned,
            scanned: result.scanned,
            updated: 0,
            downloaded: result.downloaded,
            failed: result.failed,
            skipped: result.skipped,
            progress: 100,
            indeterminate: false,
        },
    );

    Ok(result)
}

async fn enrich_single_xhs_note_before_media_download(
    app: &AppHandle,
    cookie_header: &str,
    note_id: &str,
) -> Result<BatchJobResult, String> {
    let note_ids = vec![note_id.to_string()];
    let (_, conn) = open_library(app)?;
    let targets = read_xhs_detail_targets_by_ids(&conn, &note_ids)
        .map_err(|error| format!("读取当前笔记详情状态失败：{error}"))?;
    drop(conn);

    if targets.is_empty() {
        return Ok(BatchJobResult {
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 1,
            message: "当前笔记已经有可用详情。".to_string(),
        });
    }

    emit_batch_progress(
        app,
        "media-download-progress",
        BatchJobProgress {
            phase: "fetching_detail".to_string(),
            label: "补全当前笔记".to_string(),
            detail: "下载前先读取正文、标签和完整图片/视频列表。".to_string(),
            planned: 1,
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            progress: 2,
            indeterminate: false,
        },
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("初始化小红书详情客户端失败：{error}"))?;
    let target = &targets[0];
    let mut result = BatchJobResult {
        scanned: 1,
        updated: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        message: String::new(),
    };

    match fetch_xhs_note_detail_resilient(app, &client, cookie_header, target).await {
        Ok(detail) => {
            let (_, conn) = open_library(app)?;
            upsert_xhs_note_detail(&conn, &target.id, &target.source_note_id, &detail)
                .map_err(|error| format!("写入当前笔记详情失败：{error}"))?;
            result.updated = 1;
            result.message = "当前笔记详情已补全。".to_string();
        }
        Err(XhsDetailFetchError::Gone(status_code)) => {
            let (_, conn) = open_library(app)?;
            mark_xhs_note_unavailable(&conn, &target.id, &format!("detail_http_{status_code}"))
                .map_err(|error| format!("标记失效笔记失败：{error}"))?;
            result.failed = 1;
            result.message = format!("当前笔记详情页返回 HTTP {status_code}。");
        }
        Err(XhsDetailFetchError::NeedsVerification(message)) => {
            log::warn!(
                "media_download_note_detail_verification_required note_id={} message={}",
                target.source_note_id,
                message
            );
            result.skipped = 1;
            result.message =
                "小红书要求验证码。请在弹出的 Tauri 小红书窗口完成验证后再重试。".to_string();
        }
        Err(XhsDetailFetchError::Other(error)) => {
            log::warn!(
                "media_download_note_detail_fetch_failed note_id={} error={}",
                target.source_note_id,
                error
            );
            result.failed = 1;
            result.message = format!("当前笔记详情补全失败：{error}");
        }
    }

    emit_batch_progress(
        app,
        "media-download-progress",
        BatchJobProgress {
            phase: "fetching_detail".to_string(),
            label: "补全当前笔记".to_string(),
            detail: result.message.clone(),
            planned: 1,
            scanned: result.scanned,
            updated: result.updated,
            downloaded: 0,
            failed: result.failed,
            skipped: result.skipped,
            progress: 8,
            indeterminate: false,
        },
    );

    Ok(result)
}

async fn enrich_synced_xhs_note_details(
    app: &AppHandle,
    cookie_header: &str,
    note_ids: &[String],
    scanned: usize,
    existing_skipped: usize,
) -> Result<BatchJobResult, String> {
    if note_ids.is_empty() {
        return Ok(BatchJobResult {
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            message: "没有本次新增或更新的笔记需要补全。".to_string(),
        });
    }

    let (_, conn) = open_library(app)?;
    let targets = read_xhs_detail_targets_by_ids(&conn, note_ids)
        .map_err(|error| format!("读取本次待补全笔记失败：{error}"))?;
    drop(conn);

    let planned = targets.len();
    if planned == 0 {
        return Ok(BatchJobResult {
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: note_ids.len(),
            message: "本次写入的笔记已经有可用详情。".to_string(),
        });
    }

    emit_xhs_sync_progress(
        app,
        XhsSyncProgress {
            phase: "enriching_details".to_string(),
            label: "补全笔记内容".to_string(),
            detail: format!("正在补全 {planned} 条笔记的正文、标签、作者和媒体地址。"),
            planned,
            scanned,
            fetched: scanned,
            to_sync: Some(planned),
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped,
            progress: 88,
            indeterminate: false,
        },
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("初始化小红书详情客户端失败：{error}"))?;
    let mut result = BatchJobResult {
        scanned: 0,
        updated: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        message: String::new(),
    };

    for (index, target) in targets.iter().enumerate() {
        ensure_xhs_sync_not_cancelled()?;
        match fetch_xhs_note_detail_resilient(app, &client, cookie_header, target).await {
            Ok(detail) => {
                let (_, conn) = open_library(app)?;
                upsert_xhs_note_detail(&conn, &target.id, &target.source_note_id, &detail)
                    .map_err(|error| format!("写入笔记详情失败：{error}"))?;
                result.updated += 1;
            }
            Err(XhsDetailFetchError::Gone(status_code)) => {
                let (_, conn) = open_library(app)?;
                mark_xhs_note_unavailable(&conn, &target.id, &format!("detail_http_{status_code}"))
                    .map_err(|error| format!("标记失效笔记失败：{error}"))?;
                result.failed += 1;
            }
            Err(XhsDetailFetchError::NeedsVerification(message)) => {
                log::warn!(
                    "xhs_sync_detail_verification_required note_id={} message={}",
                    target.source_note_id,
                    message
                );
                result.skipped = planned.saturating_sub(result.scanned);
                result.message = format!(
                    "小红书要求验证码，后台详情补全已暂停。已处理 {} 条，补全 {} 条，失败 {} 条。",
                    result.scanned, result.updated, result.failed
                );
                emit_xhs_sync_progress(
                    app,
                    XhsSyncProgress {
                        phase: "post_sync_verification_required".to_string(),
                        label: "需要小红书验证".to_string(),
                        detail: result.message.clone(),
                        planned,
                        scanned,
                        fetched: scanned,
                        to_sync: Some(planned),
                        written: result.scanned,
                        inserted: 0,
                        updated: result.updated,
                        skipped: result.skipped,
                        existing_skipped,
                        progress: 92,
                        indeterminate: false,
                    },
                );
                return Ok(result);
            }
            Err(XhsDetailFetchError::Other(error)) => {
                log::warn!(
                    "xhs_sync_detail_fetch_failed note_id={} error={}",
                    target.source_note_id,
                    error
                );
                result.failed += 1;
            }
        }

        result.scanned += 1;
        emit_xhs_sync_progress(
            app,
            XhsSyncProgress {
                phase: "enriching_details".to_string(),
                label: "补全笔记内容".to_string(),
                detail: format!(
                    "已补全 {} / {planned} 条，成功 {} 条，失败 {} 条。",
                    result.scanned, result.updated, result.failed
                ),
                planned,
                scanned,
                fetched: scanned,
                to_sync: Some(planned),
                written: result.scanned,
                inserted: 0,
                updated: result.updated,
                skipped: result.skipped,
                existing_skipped,
                progress: (88 + (((index + 1) * 5) / usize::max(planned, 1)).min(5)) as u8,
                indeterminate: false,
            },
        );

        if index + 1 < planned {
            std::thread::sleep(XHS_REQUEST_DELAY);
        }
    }

    result.message = format!(
        "笔记详情补全：处理 {} 条，成功 {} 条，失败 {} 条。",
        result.scanned, result.updated, result.failed
    );
    Ok(result)
}

async fn download_sync_media_targets(
    app: &AppHandle,
    cookie_header: &str,
    targets: Vec<MediaDownloadTarget>,
    phase: &str,
    label: &str,
    progress_start: u8,
    progress_span: u8,
) -> Result<BatchJobResult, String> {
    let planned = targets.len();
    if planned == 0 {
        return Ok(BatchJobResult {
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            message: "没有需要下载的媒体。".to_string(),
        });
    }

    let (paths, _) = open_library(app)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("初始化媒体下载客户端失败：{error}"))?;
    let mut result = BatchJobResult {
        scanned: 0,
        updated: 0,
        downloaded: 0,
        failed: 0,
        skipped: 0,
        message: String::new(),
    };

    emit_xhs_sync_progress(
        app,
        XhsSyncProgress {
            phase: phase.to_string(),
            label: label.to_string(),
            detail: format!("本轮准备下载 {planned} 个本地媒体资产。"),
            planned,
            scanned: 0,
            fetched: planned,
            to_sync: Some(planned),
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: progress_start,
            indeterminate: false,
        },
    );

    let concurrency = media_download_concurrency(planned);
    let mut next_target = targets.into_iter().enumerate();
    let mut active = FuturesUnordered::new();
    for _ in 0..concurrency {
        if let Some((index, target)) = next_target.next() {
            ensure_xhs_sync_not_cancelled()?;
            active.push(download_one_media_target(
                app.clone(),
                paths.media_dir.clone(),
                client.clone(),
                cookie_header.to_string(),
                index,
                target,
            ));
        }
    }

    while let Some(outcome) = active.next().await {
        if outcome.downloaded {
            result.downloaded += 1;
        } else {
            result.failed += 1;
        }
        result.scanned += 1;
        let progress = progress_start
            + ((result.scanned * progress_span as usize) / usize::max(planned, 1))
                .min(progress_span as usize) as u8;
        emit_xhs_sync_progress(
            app,
            XhsSyncProgress {
                phase: phase.to_string(),
                label: label.to_string(),
                detail: format!(
                    "并行 {} 路，已处理 {} / {planned} 个，下载 {} 个，失败 {} 个。{}",
                    concurrency, result.scanned, result.downloaded, result.failed, outcome.detail
                ),
                planned,
                scanned: result.scanned,
                fetched: planned,
                to_sync: Some(planned),
                written: result.scanned,
                inserted: 0,
                updated: 0,
                skipped: result.skipped,
                existing_skipped: 0,
                progress,
                indeterminate: false,
            },
        );

        if let Some((index, target)) = next_target.next() {
            ensure_xhs_sync_not_cancelled()?;
            active.push(download_one_media_target(
                app.clone(),
                paths.media_dir.clone(),
                client.clone(),
                cookie_header.to_string(),
                index,
                target,
            ));
        }
    }

    result.message = format!(
        "媒体下载：处理 {} 个，下载 {} 个，失败 {} 个。",
        result.scanned, result.downloaded, result.failed
    );
    Ok(result)
}

#[tauri::command]
pub(crate) async fn open_xhs_login_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(XHS_LOGIN_WINDOW_LABEL) {
        log::info!("xhs_login_window_focus_existing");
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    log::info!("xhs_login_window_open_start");
    let login_url = Url::parse(XHS_LOGIN_URL).map_err(|error| error.to_string())?;
    WebviewWindowBuilder::new(
        &app,
        XHS_LOGIN_WINDOW_LABEL,
        WebviewUrl::External(login_url),
    )
    .title("登录小红书 - XHS Collection")
    .inner_size(1040.0, 780.0)
    .min_inner_size(760.0, 560.0)
    .center()
    .focused(true)
    .build()
    .map_err(|error| format!("打开小红书登录窗口失败：{error}"))?;

    log::info!("xhs_login_window_open_done");
    Ok(())
}

#[tauri::command]
pub(crate) async fn read_xhs_login_cookies(app: AppHandle) -> Result<XhsSessionTestResult, String> {
    let started_at = Instant::now();
    log::info!("xhs_cookie_read_command_start");
    let window = app
        .get_webview_window(XHS_LOGIN_WINDOW_LABEL)
        .ok_or_else(|| "请先打开内置登录窗口，并在里面完成小红书登录。".to_string())?;

    let (cookie_header, used_document_cookie_fallback) = read_xhs_cookie_header(window.clone())?;
    log::info!(
        "xhs_cookie_read_command_got_header elapsed_ms={} fallback={} key_count={}",
        started_at.elapsed().as_millis(),
        used_document_cookie_fallback,
        cookie_keys(&cookie_header).len()
    );

    if cookie_header.trim().is_empty() {
        return Err("没有从内置登录窗口读到小红书 Cookie。请确认登录完成后再重试。".to_string());
    }

    let mut result = test_xhs_cookie(&cookie_header).await?;
    if let Ok(account) = extract_xhs_account_info(&window) {
        merge_account_info_into_session_result(&mut result, account);
    }
    if result.ok {
        save_xhs_session(&app, &cookie_header, &result)?;
    }
    if used_document_cookie_fallback {
        result.message = format!(
            "已使用 document.cookie 兜底读取，可能缺少 HttpOnly Cookie。{}",
            result.message
        );
    }

    log::info!(
        "xhs_cookie_read_command_done elapsed_ms={} ok={} status={}",
        started_at.elapsed().as_millis(),
        result.ok,
        result.status_code
    );
    Ok(result)
}

fn read_xhs_cookie_header<R: Runtime>(window: WebviewWindow<R>) -> Result<(String, bool), String> {
    let (cookie_tx, cookie_rx) = mpsc::channel();
    let cookie_window = window.clone();

    std::thread::spawn(move || {
        log::info!("xhs_cookie_store_read_start");
        let result = read_cookie_header_from_store(cookie_window);
        match &result {
            Ok(cookie_header) => log::info!(
                "xhs_cookie_store_read_done key_count={}",
                cookie_keys(cookie_header).len()
            ),
            Err(error) => log::warn!("xhs_cookie_store_read_error {error}"),
        }
        let _ = cookie_tx.send(result);
    });

    match cookie_rx.recv_timeout(XHS_COOKIE_STORE_TIMEOUT) {
        Ok(Ok(cookie_header)) if !cookie_header.trim().is_empty() => Ok((cookie_header, false)),
        Ok(Ok(_)) => read_document_cookie_from_window(&window).map(|cookie| (cookie, true)),
        Ok(Err(store_error)) => match read_document_cookie_from_window(&window) {
            Ok(cookie_header) if !cookie_header.trim().is_empty() => Ok((cookie_header, true)),
            _ => Err(store_error),
        },
        Err(mpsc::RecvTimeoutError::Timeout) => {
            log::warn!("xhs_cookie_store_read_timeout");
            match read_document_cookie_from_window(&window) {
                Ok(cookie_header) if !cookie_header.trim().is_empty() => Ok((cookie_header, true)),
                _ => Err(
                    "读取内置登录窗口 Cookie 超时。请关闭登录窗口重开再试，或先用手动 Cookie 兜底。"
                        .to_string(),
                ),
            }
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("读取内置登录窗口 Cookie 失败：读取线程已退出。".to_string())
        }
    }
}

fn read_cookie_header_from_store<R: Runtime>(window: WebviewWindow<R>) -> Result<String, String> {
    let mut seen_cookie_names = HashSet::new();
    let mut cookie_pairs = Vec::new();

    for url in XHS_COOKIE_URLS {
        let parsed_url = Url::parse(url).map_err(|error| error.to_string())?;
        let cookies = window
            .cookies_for_url(parsed_url)
            .map_err(|error| format!("读取内置登录窗口 Cookie 失败：{error}"))?;

        for cookie in cookies {
            let name = cookie.name().trim();
            let value = cookie.value().trim();
            if name.is_empty() || value.is_empty() || !seen_cookie_names.insert(name.to_string()) {
                continue;
            }
            cookie_pairs.push(format!("{name}={value}"));
        }
    }

    Ok(cookie_pairs.join("; "))
}

fn read_document_cookie_from_window<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<String, String> {
    let (document_tx, document_rx) = mpsc::channel();
    log::info!("xhs_document_cookie_read_start");
    window
        .eval_with_callback(
            "(() => { try { return document.cookie || ''; } catch (_) { return ''; } })()",
            move |payload| {
                let _ = document_tx.send(payload);
            },
        )
        .map_err(|error| format!("读取 document.cookie 失败：{error}"))?;

    let payload = document_rx
        .recv_timeout(XHS_DOCUMENT_COOKIE_TIMEOUT)
        .map_err(|_| "读取 document.cookie 超时。请确认登录窗口仍打开在小红书页面。".to_string())?;

    let value: serde_json::Value = serde_json::from_str(&payload)
        .unwrap_or_else(|_| serde_json::Value::String(payload.clone()));
    let cookie_header = value
        .as_str()
        .unwrap_or(payload.as_str())
        .trim()
        .to_string();

    if cookie_header.is_empty() {
        return Err("document.cookie 没有返回可用 Cookie。请确认登录窗口已完成登录。".to_string());
    }

    log::info!(
        "xhs_document_cookie_read_done key_count={}",
        cookie_keys(&cookie_header).len()
    );
    Ok(cookie_header)
}

#[tauri::command]
pub(crate) async fn test_xhs_session(
    app: AppHandle,
    cookie: String,
) -> Result<XhsSessionTestResult, String> {
    let result = test_xhs_cookie(&cookie).await?;
    if result.ok {
        save_xhs_session(&app, &cookie, &result)?;
    }
    Ok(result)
}

async fn test_xhs_cookie(cookie: &str) -> Result<XhsSessionTestResult, String> {
    let started_at = Instant::now();
    let cookie = cookie.trim();
    if cookie.is_empty() {
        return Err("请先粘贴从你自己浏览器里复制的小红书 Cookie。".to_string());
    }

    if cookie.len() > 20000 {
        return Err("Cookie 太长，当前连接测试最多接受 20000 个字符。".to_string());
    }

    let cookie_keys = cookie_keys(cookie);
    log::info!("xhs_session_test_start key_count={}", cookie_keys.len());
    let has_session_marker = cookie_keys.iter().any(|key| {
        matches!(
            key.as_str(),
            "web_session" | "a1" | "websectiga" | "x-user-id-creator.xiaohongshu.com"
        )
    });

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .get("https://www.xiaohongshu.com/explore")
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .header(ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
        .header(REFERER, "https://www.xiaohongshu.com/")
        .header(COOKIE, cookie)
        .send()
        .await
        .map_err(|error| format!("请求小红书失败：{error}"))?;

    let status = response.status();
    let final_url = response.url().to_string();
    let html = response
        .text()
        .await
        .map_err(|error| format!("读取小红书响应失败：{error}"))?;

    let page_title = extract_title(&html);
    let login_markers = ["登录", "login", "验证码", "二维码", "手机号登录"];
    let has_login_marker = login_markers.iter().any(|marker| html.contains(marker));
    let account = account_info_from_cookie_and_html(cookie, &html);
    let account_hint = account_hint(&cookie_keys, &html, account.user_id.as_deref());
    let ok = status.is_success() && has_session_marker && !final_url.contains("login");

    let message = if ok && !has_login_marker {
        "连接测试通过：Cookie 形态正常，小红书首页可访问。下一步可以尝试收藏接口。".to_string()
    } else if ok {
        "基础连接通过：Cookie 形态正常，小红书页面可访问。首页 HTML 仍包含登录文案，下一步用收藏接口做最终确认。"
            .to_string()
    } else if !has_session_marker {
        "没有检测到常见登录 Cookie（例如 web_session / a1 / websectiga），请确认是在登录后复制。"
            .to_string()
    } else {
        format!("连接测试未通过：小红书返回 HTTP {}。", status.as_u16())
    };

    log::info!(
        "xhs_session_test_done elapsed_ms={} ok={} status={} final_url={}",
        started_at.elapsed().as_millis(),
        ok,
        status.as_u16(),
        final_url
    );
    Ok(XhsSessionTestResult {
        ok,
        status_code: status.as_u16(),
        final_url,
        page_title,
        account_hint,
        account_id: account.user_id,
        account_name: account.nickname,
        avatar_url: account.avatar_url,
        cookie_keys,
        checked_at: chrono::Utc::now().to_rfc3339(),
        message,
    })
}

#[tauri::command]
pub(crate) fn cancel_xhs_sync(app: AppHandle) -> Result<(), String> {
    request_xhs_sync_cancel();
    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "cancel_requested".to_string(),
            label: "正在终止同步".to_string(),
            detail: "已收到终止请求，当前步骤结束后会停止同步。".to_string(),
            planned: 0,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 100,
            indeterminate: true,
        },
    );
    Ok(())
}

#[tauri::command]
pub(crate) async fn sync_xhs_favorites(
    app: AppHandle,
    input: XhsFavoriteSyncInput,
) -> Result<XhsFavoriteSyncResult, String> {
    sync_xhs_collection_notes(app, input, "note", "favorites", "收藏").await
}

#[tauri::command]
pub(crate) async fn sync_xhs_files(
    app: AppHandle,
    input: XhsFavoriteSyncInput,
) -> Result<XhsFavoriteSyncResult, String> {
    sync_xhs_collection_notes(app, input, "file", "files", "文件").await
}

async fn sync_xhs_collection_notes(
    app: AppHandle,
    input: XhsFavoriteSyncInput,
    sub_tab: &str,
    checkpoint_mode: &str,
    sync_label: &str,
) -> Result<XhsFavoriteSyncResult, String> {
    reset_xhs_sync_cancel();
    let started_at = Instant::now();
    let requested_max = input.max_count.filter(|count| *count > 0);
    let resume = input.resume.unwrap_or(false);
    let full_sync = input.full_sync.unwrap_or(false) || resume;
    let planned_count = requested_max.unwrap_or(0);
    let plan_detail = requested_max.map_or_else(
        || {
            if full_sync {
                if resume {
                    format!("从当前{sync_label}页位置继续完整同步，直到页面末尾。")
                } else {
                    format!("完整同步会读取到{sync_label}页末尾，并校准远端缺失状态。")
                }
            } else {
                format!("快速同步只读取最近{sync_label}；遇到已同步笔记或总数未变化就停止。")
            }
        },
        |count| format!("本次最多读取 {count} 条{sync_label}。"),
    );
    log::info!(
        "xhs_native_sync_start sub_tab={} requested_max={:?} resume={} full_sync={} max_scroll_attempts={}",
        sub_tab,
        requested_max,
        resume,
        full_sync,
        XHS_FAVORITES_MAX_SCROLL_ATTEMPTS
    );
    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "preparing".to_string(),
            label: "准备同步".to_string(),
            detail: plan_detail,
            planned: planned_count,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 5,
            indeterminate: false,
        },
    );

    let window = match app.get_webview_window(XHS_LOGIN_WINDOW_LABEL) {
        Some(window) => window,
        None => {
            log::info!("xhs_native_sync_open_login_window_for_saved_session");
            open_xhs_login_window(app.clone()).await?;
            std::thread::sleep(Duration::from_millis(1200));
            app.get_webview_window(XHS_LOGIN_WINDOW_LABEL)
                .ok_or_else(|| "无法打开内置登录窗口。请手动打开登录窗口后再同步。".to_string())?
        }
    };
    log::info!("xhs_native_sync_window_found");

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "checking_session".to_string(),
            label: "读取登录态".to_string(),
            detail: "正在从内置登录窗口读取本地 Cookie。".to_string(),
            planned: planned_count,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 12,
            indeterminate: false,
        },
    );
    log::info!("xhs_native_sync_cookie_read_start");
    let mut cookie_header = match read_xhs_cookie_header(window.clone()) {
        Ok((cookie_header, _)) if !cookie_header.trim().is_empty() => {
            log::info!(
                "xhs_native_sync_cookie_read_done key_count={}",
                cookie_keys(&cookie_header).len()
            );
            cookie_header
        }
        Ok(_) => {
            log::warn!("xhs_native_sync_cookie_read_empty");
            restore_saved_xhs_cookie_to_window(&app, &window)?
        }
        Err(error) => {
            log::warn!("xhs_native_sync_cookie_read_failed {error}");
            restore_saved_xhs_cookie_to_window(&app, &window)?
        }
    };

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "checking_session".to_string(),
            label: "验证登录态".to_string(),
            detail: "正在确认当前账号可以访问小红书页面。".to_string(),
            planned: planned_count,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 22,
            indeterminate: false,
        },
    );
    let mut session = test_xhs_cookie(&cookie_header).await?;
    if !session.ok {
        if let Some(saved_cookie) = read_saved_xhs_cookie(&app)? {
            if saved_cookie.trim() != cookie_header.trim() {
                let saved_session = test_xhs_cookie(&saved_cookie).await?;
                if saved_session.ok {
                    let injected = inject_xhs_cookie_header(&window, &saved_cookie)?;
                    log::info!(
                        "xhs_native_sync_saved_cookie_used key_count={} injected={}",
                        cookie_keys(&saved_cookie).len(),
                        injected
                    );
                    cookie_header = saved_cookie;
                    session = saved_session;
                }
            }
        }
    }
    if let Ok(account) = extract_xhs_account_info(&window) {
        merge_account_info_into_session_result(&mut session, account);
    }
    let active_account_before = active_local_xhs_account_id(&app).unwrap_or_default();
    if !session.ok {
        return Err("当前登录态还不能同步收藏。请重新登录后再试。".to_string());
    }
    save_xhs_session(&app, &cookie_header, &session)?;

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "detecting_account".to_string(),
            label: "识别账号".to_string(),
            detail: "正在打开小红书首页并读取当前用户。".to_string(),
            planned: planned_count,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 35,
            indeterminate: false,
        },
    );
    log::info!("xhs_native_sync_user_detect_start");
    let user_id = extract_xhs_user_id(&window).or_else(|error| {
        session
            .account_id
            .clone()
            .ok_or_else(|| format!("无法识别当前小红书用户：{error}"))
    })?;
    if session.account_id.as_deref() != Some(user_id.as_str()) {
        session.account_id = Some(user_id.clone());
    }
    save_xhs_session(&app, &cookie_header, &session)?;
    let cleaned_albums = {
        let (_, conn) = open_library(&app)?;
        cleanup_invalid_xhs_albums(&conn, &user_id)
            .map_err(|error| format!("清理无效专辑失败：{error}"))?
    };
    if cleaned_albums > 0 {
        log::warn!("xhs_album_invalid_rows_cleaned count={cleaned_albums}");
    }
    let account_changed = active_account_before.as_deref() != Some(user_id.as_str());
    let account_name = session.account_name.as_deref().unwrap_or(user_id.as_str());
    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "detecting_account".to_string(),
            label: if account_changed {
                "已切换本地账号".to_string()
            } else {
                "账号已确认".to_string()
            },
            detail: if account_changed {
                format!("检测到登录窗口账号为「{account_name}」，已切换到对应的本地账号库。")
            } else {
                format!("当前小红书账号为「{account_name}」。")
            },
            planned: planned_count,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 38,
            indeterminate: false,
        },
    );
    emit_library_changed(
        &app,
        if account_changed {
            format!("已切换到当前小红书账号：{account_name}")
        } else {
            format!("已确认当前小红书账号：{account_name}")
        },
    );
    log::info!("xhs_native_sync_user_detect_done");
    let (_, conn) = open_library(&app)?;
    let existing_note_ids = read_xhs_existing_note_ids(&conn)
        .map_err(|error| format!("读取本地收藏索引失败：{error}"))?;
    let checkpoint = read_xhs_sync_checkpoint(&conn, &user_id, checkpoint_mode)
        .map_err(|error| format!("读取同步断点失败：{error}"))?;
    if let Some(checkpoint) = &checkpoint {
        log::info!(
            "xhs_sync_checkpoint_loaded anchor={:?} reached_end={} scanned_count={} remote_display_count={:?}",
            checkpoint.anchor_source_note_id,
            checkpoint.reached_end,
            checkpoint.scanned_count,
            checkpoint.remote_display_count
        );
    }

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "fetching_favorites".to_string(),
            label: format!("读取{sync_label}列表"),
            detail: if resume {
                format!("正在从当前{sync_label}页位置继续读取卡片。")
            } else if full_sync {
                format!("正在完整读取{sync_label}页，直到页面末尾。")
            } else {
                format!("正在快速读取最近{sync_label}，遇到已同步笔记就停止。")
            },
            planned: planned_count,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 48,
            indeterminate: requested_max.is_none(),
        },
    );
    log::info!("xhs_native_sync_collect_start");
    let collect_result = collect_xhs_favorites(
        &app,
        &window,
        &user_id,
        sub_tab,
        requested_max,
        resume,
        full_sync,
        &existing_note_ids,
        checkpoint.as_ref(),
    )?;
    let raw_notes = &collect_result.notes;
    let fetched = raw_notes.len();
    log::info!(
        "xhs_native_sync_collect_done scanned={} fetched={} existing_skipped={} limit_reached={} reached_end={} reason={}",
        collect_result.scanned,
        fetched,
        collect_result.existing_skipped,
        collect_result.limit_reached,
        collect_result.reached_end,
        collect_result.stopped_reason
    );

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "writing_library".to_string(),
            label: "写入本地库".to_string(),
            detail: if fetched == 0 {
                format!(
                    "扫描 {} 条，跳过已存在 {} 条；没有新的可写入收藏（{}）。",
                    collect_result.scanned,
                    collect_result.existing_skipped,
                    collect_result.stopped_reason
                )
            } else {
                format!(
                    "扫描 {} 条，需写入 {fetched} 条，跳过已存在 {} 条；正在写入 SQLite。",
                    collect_result.scanned, collect_result.existing_skipped
                )
            },
            planned: planned_count,
            scanned: collect_result.scanned,
            fetched: collect_result.scanned,
            to_sync: Some(fetched),
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: collect_result.existing_skipped,
            progress: 72,
            indeterminate: false,
        },
    );

    let summary = upsert_xhs_favorite_notes(
        &conn,
        raw_notes,
        Some(&app),
        planned_count,
        fetched,
        collect_result.scanned,
        collect_result.existing_skipped,
    )
    .map_err(|error| format!("写入本地收藏库失败：{error}"))?;
    mark_seen_xhs_notes_available(
        &conn,
        &collect_result.seen_note_ids,
        &collect_result.favorite_positions,
    )
    .map_err(|error| format!("更新远端可见状态失败：{error}"))?;
    let remote_unreturned =
        remote_unreturned_count(collect_result.scanned, collect_result.remote_display_count);
    let can_trust_complete_remote_set = remote_unreturned.unwrap_or(0) == 0;
    let remote_missing = if checkpoint_mode == "favorites"
        && full_sync
        && !resume
        && collect_result.reached_end
        && !collect_result.limit_reached
        && can_trust_complete_remote_set
    {
        mark_unseen_xhs_notes_missing(&conn, &collect_result.seen_note_ids)
            .map_err(|error| format!("更新远端缺失状态失败：{error}"))?
    } else {
        0
    };
    upsert_xhs_sync_checkpoint(&conn, &user_id, checkpoint_mode, &collect_result)
        .map_err(|error| format!("保存同步断点失败：{error}"))?;
    record_xhs_sync_run(&conn, &user_id, &summary, &collect_result, remote_missing)
        .map_err(|error| format!("记录同步运行失败：{error}"))?;
    mark_active_profile_synced(&app, &user_id);
    drop(conn);

    let completion_message = sync_completion_message(
        fetched,
        &summary,
        collect_result.scanned,
        collect_result.existing_skipped,
        remote_missing,
        collect_result.limit_reached,
        requested_max,
        collect_result.remote_display_count,
        full_sync && !resume && collect_result.reached_end,
        &collect_result.stopped_reason,
    );
    let post_sync_started = start_xhs_post_sync_background(
        app.clone(),
        cookie_header,
        summary.note_ids.clone(),
        collect_result.scanned,
        collect_result.existing_skipped,
    );
    let completion_message = if summary.note_ids.is_empty() {
        completion_message
    } else if post_sync_started {
        format!("{completion_message} 已开始后台补全正文、封面和首批 {XHS_DOWNLOAD_DEFAULT_LIMIT} 个图片/视频。")
    } else {
        format!("{completion_message} 后台补全任务已在运行，本次先更新收藏索引。")
    };

    log::info!(
        "xhs_native_sync_done elapsed_ms={} scanned={} fetched={} inserted={} updated={} skipped={} existing_skipped={} remote_missing={} post_sync_started={}",
        started_at.elapsed().as_millis(),
        collect_result.scanned,
        fetched,
        summary.inserted,
        summary.updated,
        summary.skipped,
        collect_result.existing_skipped,
        remote_missing,
        post_sync_started
    );

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "completed".to_string(),
            label: "同步完成".to_string(),
            detail: completion_message.clone(),
            planned: planned_count,
            scanned: collect_result.scanned,
            fetched: collect_result.scanned,
            to_sync: Some(fetched),
            written: summary.inserted + summary.updated + summary.skipped,
            inserted: summary.inserted,
            updated: summary.updated,
            skipped: summary.skipped,
            existing_skipped: collect_result.existing_skipped,
            progress: 100,
            indeterminate: false,
        },
    );

    Ok(XhsFavoriteSyncResult {
        scanned: collect_result.scanned,
        fetched,
        inserted: summary.inserted,
        updated: summary.updated,
        skipped: summary.skipped,
        existing_skipped: collect_result.existing_skipped,
        remote_missing,
        remote_display_count: collect_result.remote_display_count,
        remote_unreturned_count: remote_unreturned,
        limit_reached: collect_result.limit_reached,
        full_sync,
        details_updated: 0,
        details_failed: 0,
        covers_downloaded: 0,
        covers_failed: 0,
        media_downloaded: 0,
        media_failed: 0,
        message: completion_message,
    })
}

#[tauri::command]
pub(crate) async fn sync_xhs_albums(
    app: AppHandle,
    input: XhsAlbumSyncInput,
) -> Result<XhsAlbumSyncResult, String> {
    reset_xhs_sync_cancel();
    let started_at = Instant::now();
    let max_albums = input.max_albums.filter(|count| *count > 0).unwrap_or(80);
    let max_notes_per_album = input
        .max_notes_per_album
        .filter(|count| *count > 0)
        .unwrap_or(400);
    log::info!(
        "xhs_album_sync_start max_albums={} max_notes_per_album={}",
        max_albums,
        max_notes_per_album
    );

    let window = match app.get_webview_window(XHS_LOGIN_WINDOW_LABEL) {
        Some(window) => window,
        None => {
            open_xhs_login_window(app.clone()).await?;
            std::thread::sleep(Duration::from_millis(1200));
            app.get_webview_window(XHS_LOGIN_WINDOW_LABEL)
                .ok_or_else(|| {
                    "无法打开内置登录窗口。请手动打开登录窗口后再同步专辑。".to_string()
                })?
        }
    };

    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "syncing_albums".to_string(),
            label: "读取专辑".to_string(),
            detail: "正在读取小红书收藏专辑列表。".to_string(),
            planned: max_albums,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 8,
            indeterminate: true,
        },
    );

    let mut cookie_header = match read_xhs_cookie_header(window.clone()) {
        Ok((cookie_header, _)) if !cookie_header.trim().is_empty() => cookie_header,
        _ => restore_saved_xhs_cookie_to_window(&app, &window)?,
    };
    let mut session = test_xhs_cookie(&cookie_header).await?;
    if !session.ok {
        if let Some(saved_cookie) = read_saved_xhs_cookie(&app)? {
            let saved_session = test_xhs_cookie(&saved_cookie).await?;
            if saved_session.ok {
                inject_xhs_cookie_header(&window, &saved_cookie)?;
                cookie_header = saved_cookie;
                session = saved_session;
            }
        }
    }
    if !session.ok {
        return Err("当前登录态还不能同步专辑。请重新登录后再试。".to_string());
    }
    let active_account_before = active_local_xhs_account_id(&app).unwrap_or_default();
    if let Ok(account) = extract_xhs_account_info(&window) {
        merge_account_info_into_session_result(&mut session, account);
    }
    save_xhs_session(&app, &cookie_header, &session)?;

    let user_id = extract_xhs_user_id(&window).or_else(|error| {
        session
            .account_id
            .clone()
            .ok_or_else(|| format!("无法识别当前小红书用户：{error}"))
    })?;
    if session.account_id.as_deref() != Some(user_id.as_str()) {
        session.account_id = Some(user_id.clone());
    }
    save_xhs_session(&app, &cookie_header, &session)?;
    let cleaned_albums = {
        let (_, conn) = open_library(&app)?;
        cleanup_invalid_xhs_albums(&conn, &user_id)
            .map_err(|error| format!("清理无效专辑失败：{error}"))?
    };
    if cleaned_albums > 0 {
        log::warn!("xhs_album_invalid_rows_cleaned count={cleaned_albums}");
    }
    let account_changed = active_account_before.as_deref() != Some(user_id.as_str());
    let account_name = session.account_name.as_deref().unwrap_or(user_id.as_str());
    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "detecting_account".to_string(),
            label: if account_changed {
                "已切换本地账号".to_string()
            } else {
                "账号已确认".to_string()
            },
            detail: if account_changed {
                format!("检测到登录窗口账号为「{account_name}」，已切换到对应的本地账号库。")
            } else {
                format!("当前小红书账号为「{account_name}」。")
            },
            planned: max_albums,
            scanned: 0,
            fetched: 0,
            to_sync: None,
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped: 0,
            progress: 12,
            indeterminate: false,
        },
    );
    emit_library_changed(
        &app,
        if account_changed {
            format!("已切换到当前小红书账号：{account_name}")
        } else {
            format!("已确认当前小红书账号：{account_name}")
        },
    );

    let albums = collect_xhs_albums(&window, &user_id, max_albums)?;
    let mut result = XhsAlbumSyncResult {
        albums_scanned: albums.len(),
        albums_updated: 0,
        notes_scanned: 0,
        notes_linked: 0,
        notes_inserted: 0,
        notes_updated: 0,
        duplicate_notes: 0,
        skipped: 0,
        message: String::new(),
    };
    if albums.is_empty() {
        result.message =
            "没有读取到收藏专辑。可能当前账号没有专辑，或小红书专辑页面结构发生变化。".to_string();
        emit_xhs_sync_progress(
            &app,
            XhsSyncProgress {
                phase: "albums_completed".to_string(),
                label: "没有可同步的专辑".to_string(),
                detail: result.message.clone(),
                planned: 0,
                scanned: 0,
                fetched: 0,
                to_sync: Some(0),
                written: 0,
                inserted: 0,
                updated: 0,
                skipped: 0,
                existing_skipped: 0,
                progress: 100,
                indeterminate: false,
            },
        );
        return Ok(result);
    }

    for (index, album) in albums.iter().enumerate() {
        ensure_xhs_sync_not_cancelled()?;
        emit_xhs_sync_progress(
            &app,
            XhsSyncProgress {
                phase: "syncing_albums".to_string(),
                label: "同步专辑".to_string(),
                detail: format!(
                    "正在同步专辑 {} / {}：{}",
                    index + 1,
                    albums.len(),
                    album.name
                ),
                planned: albums.len(),
                scanned: index + 1,
                fetched: result.notes_scanned,
                to_sync: album.note_count,
                written: result.notes_linked,
                inserted: result.notes_inserted,
                updated: result.notes_updated,
                skipped: result.skipped,
                existing_skipped: result.duplicate_notes,
                progress: (12 + (((index + 1) * 82) / usize::max(albums.len(), 1)).min(82)) as u8,
                indeterminate: false,
            },
        );

        let Some(source_url) = album.source_url.as_deref() else {
            result.skipped += 1;
            log::warn!(
                "xhs_album_sync_skip_without_url album_id={} name={}",
                album.source_album_id,
                album.name
            );
            continue;
        };
        let album_id = {
            let (_, conn) = open_library(&app)?;
            upsert_xhs_album(&conn, &user_id, album)
                .map_err(|error| format!("写入专辑失败：{error}"))?
        };
        result.albums_updated += 1;

        let notes = collect_xhs_album_notes(&window, source_url, max_notes_per_album)?;
        if notes.is_empty() {
            result.skipped += 1;
            log::info!(
                "xhs_album_sync_empty_album album_id={} name={}",
                album.source_album_id,
                album.name
            );
            continue;
        }
        let source_note_ids = notes.iter().filter_map(xhs_note_id).collect::<Vec<_>>();
        let duplicate_count = {
            let (_, conn) = open_library(&app)?;
            count_existing_xhs_notes(&conn, &source_note_ids)
                .map_err(|error| format!("统计专辑重复笔记失败：{error}"))?
        };
        let summary = {
            let (_, conn) = open_library(&app)?;
            upsert_xhs_favorite_notes(
                &conn,
                &notes,
                None,
                notes.len(),
                notes.len(),
                notes.len(),
                0,
            )
            .map_err(|error| format!("写入专辑笔记失败：{error}"))?
        };
        let linked = {
            let (_, conn) = open_library(&app)?;
            upsert_album_note_links(&conn, &album_id, &summary.note_ids)
                .map_err(|error| format!("写入专辑关系失败：{error}"))?
        };
        result.notes_scanned += notes.len();
        result.notes_linked += linked;
        result.notes_inserted += summary.inserted;
        result.notes_updated += summary.updated;
        result.duplicate_notes += duplicate_count;
        result.skipped += summary.skipped;
    }

    result.message = format!(
        "专辑同步完成：读取 {} 个专辑，关联 {} 条笔记；新增 {} 条，更新 {} 条，已有去重 {} 条，空/无链接跳过 {} 个。耗时 {} 秒。",
        result.albums_scanned,
        result.notes_linked,
        result.notes_inserted,
        result.notes_updated,
        result.duplicate_notes,
        result.skipped,
        started_at.elapsed().as_secs()
    );
    emit_xhs_sync_progress(
        &app,
        XhsSyncProgress {
            phase: "albums_completed".to_string(),
            label: "专辑同步完成".to_string(),
            detail: result.message.clone(),
            planned: result.albums_scanned,
            scanned: result.albums_scanned,
            fetched: result.notes_scanned,
            to_sync: Some(result.notes_linked),
            written: result.notes_linked,
            inserted: result.notes_inserted,
            updated: result.notes_updated,
            skipped: result.skipped,
            existing_skipped: result.duplicate_notes,
            progress: 100,
            indeterminate: false,
        },
    );
    emit_library_changed(&app, result.message.clone());
    Ok(result)
}

fn start_xhs_post_sync_background(
    app: AppHandle,
    cookie_header: String,
    note_ids: Vec<String>,
    scanned: usize,
    existing_skipped: usize,
) -> bool {
    if note_ids.is_empty() {
        return false;
    }

    let running = XHS_POST_SYNC_RUNNING.get_or_init(|| Mutex::new(false));
    {
        let Ok(mut is_running) = running.lock() else {
            return false;
        };
        if *is_running {
            return false;
        }
        *is_running = true;
    }

    tauri::async_runtime::spawn(async move {
        let result =
            run_xhs_post_sync_background(&app, &cookie_header, note_ids, scanned, existing_skipped)
                .await;
        if let Err(error) = result {
            log::warn!("xhs_post_sync_background_failed {error}");
            emit_xhs_sync_progress(
                &app,
                XhsSyncProgress {
                    phase: "post_sync_failed".to_string(),
                    label: "后台补全失败".to_string(),
                    detail: error.clone(),
                    planned: 0,
                    scanned: 0,
                    fetched: 0,
                    to_sync: None,
                    written: 0,
                    inserted: 0,
                    updated: 0,
                    skipped: 0,
                    existing_skipped: 0,
                    progress: 100,
                    indeterminate: false,
                },
            );
            emit_library_changed(&app, format!("后台补全失败：{error}"));
        }

        if let Some(running) = XHS_POST_SYNC_RUNNING.get() {
            if let Ok(mut is_running) = running.lock() {
                *is_running = false;
            }
        }
    });

    true
}

async fn run_xhs_post_sync_background(
    app: &AppHandle,
    cookie_header: &str,
    note_ids: Vec<String>,
    scanned: usize,
    existing_skipped: usize,
) -> Result<(), String> {
    let started_at = Instant::now();
    let planned = note_ids.len();
    log::info!("xhs_post_sync_background_start note_count={planned}");
    emit_xhs_sync_progress(
        app,
        XhsSyncProgress {
            phase: "post_sync_preparing".to_string(),
            label: "后台补全准备".to_string(),
            detail: format!("索引已写入，正在后台补全 {planned} 条收藏的正文、封面和首批媒体。"),
            planned,
            scanned,
            fetched: scanned,
            to_sync: Some(planned),
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped,
            progress: 2,
            indeterminate: false,
        },
    );

    let detail_result =
        enrich_synced_xhs_note_details(app, cookie_header, &note_ids, scanned, existing_skipped)
            .await?;

    let note_id_set = note_ids.iter().cloned().collect::<HashSet<_>>();
    let (_, conn) = open_library(app)?;
    let cover_targets =
        read_media_download_targets_filtered(&conn, usize::MAX, &["cover"], Some(&note_id_set))
            .map_err(|error| format!("读取待下载封面失败：{error}"))?;
    let initial_media_targets = read_media_download_targets_filtered(
        &conn,
        XHS_DOWNLOAD_DEFAULT_LIMIT,
        &["image", "video"],
        Some(&note_id_set),
    )
    .map_err(|error| format!("读取首批待下载媒体失败：{error}"))?;
    drop(conn);

    let cover_result = download_sync_media_targets(
        app,
        cookie_header,
        cover_targets,
        "downloading_covers",
        "下载轻量封面",
        93,
        3,
    )
    .await?;
    let media_result = download_sync_media_targets(
        app,
        cookie_header,
        initial_media_targets,
        "downloading_initial_media",
        "下载首批媒体",
        96,
        3,
    )
    .await?;

    let completion_message = sync_completion_with_assets(
        "后台补全完成。".to_string(),
        &detail_result,
        &cover_result,
        &media_result,
    );
    emit_xhs_sync_progress(
        app,
        XhsSyncProgress {
            phase: "post_sync_completed".to_string(),
            label: "后台补全完成".to_string(),
            detail: completion_message.clone(),
            planned,
            scanned,
            fetched: scanned,
            to_sync: Some(planned),
            written: detail_result.scanned + cover_result.scanned + media_result.scanned,
            inserted: 0,
            updated: detail_result.updated,
            skipped: detail_result.skipped + cover_result.skipped + media_result.skipped,
            existing_skipped,
            progress: 100,
            indeterminate: false,
        },
    );
    emit_library_changed(app, completion_message.clone());
    log::info!(
        "xhs_post_sync_background_done elapsed_ms={} details_updated={} details_failed={} covers_downloaded={} covers_failed={} media_downloaded={} media_failed={}",
        started_at.elapsed().as_millis(),
        detail_result.updated,
        detail_result.failed,
        cover_result.downloaded,
        cover_result.failed,
        media_result.downloaded,
        media_result.failed
    );
    Ok(())
}

fn extract_xhs_user_id<R: Runtime>(window: &WebviewWindow<R>) -> Result<String, String> {
    extract_xhs_account_info(window)?
        .user_id
        .ok_or_else(|| "没有从小红书页面识别到当前账号 ID。请确认登录完成后再试。".to_string())
}

fn extract_xhs_account_info<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<XhsAccountInfo, String> {
    navigate_xhs_window(window, "https://www.xiaohongshu.com/explore")?;
    wait_for_xhs_window(
        window,
        "账号信息",
        "(() => Boolean(window.__INITIAL_STATE__ && window.__INITIAL_STATE__.user))()",
        XHS_PAGE_READY_TIMEOUT,
    )?;

    let value = eval_xhs_window_value(window, &script_with_unwrap(XHS_SELF_INFO_SCRIPT))?;
    let user_id = value
        .get("userId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let nickname = value
        .get("nickname")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let avatar_url = value
        .get("avatarUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.starts_with("http"))
        .map(ToString::to_string);

    log::info!("xhs_native_user_id_detected");
    Ok(XhsAccountInfo {
        user_id,
        nickname,
        avatar_url,
    })
}

#[derive(Debug, Clone)]
struct XhsAlbumDraft {
    source_album_id: String,
    name: String,
    description: String,
    source_url: Option<String>,
    cover_url: Option<String>,
    note_count: Option<usize>,
    raw_json: String,
}

fn collect_xhs_albums<R: Runtime>(
    window: &WebviewWindow<R>,
    user_id: &str,
    max_albums: usize,
) -> Result<Vec<XhsAlbumDraft>, String> {
    let favorite_roots = [
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=fav"),
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=collect"),
    ];
    let fallback_tab_urls = [
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=fav&subTab=board"),
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=fav&subTab=album"),
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=collect&subTab=board"),
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=collect&subTab=album"),
    ];
    let collection_tab_labels: &[&[&str]] = &[&["专辑"]];
    let extraction_script = script_with_unwrap(XHS_ALBUMS_SCRIPT);
    let mut albums = Vec::new();
    let mut seen = HashSet::new();

    for profile_url in favorite_roots {
        ensure_xhs_sync_not_cancelled()?;
        navigate_xhs_window(window, &profile_url)?;
        wait_for_xhs_window(
            window,
            "收藏页面",
            "(() => Boolean(document.body && document.body.innerText && window.__INITIAL_STATE__))()",
            XHS_PAGE_READY_TIMEOUT,
        )?;

        let _ = click_xhs_collection_tab(window, &["收藏"])?;
        for labels in collection_tab_labels {
            ensure_xhs_sync_not_cancelled()?;
            let clicked = click_xhs_collection_tab(window, labels)?;
            log::info!(
                "xhs_album_collect_subtab labels={} clicked={}",
                labels.join("/"),
                clicked
            );
            if collect_xhs_album_candidates_on_current_page(
                window,
                max_albums,
                &extraction_script,
                &mut albums,
                &mut seen,
            )? {
                return Ok(albums);
            }
            if !albums.is_empty() {
                return Ok(albums);
            }
        }

        if !albums.is_empty() {
            break;
        }
    }

    if albums.is_empty() {
        for profile_url in fallback_tab_urls {
            ensure_xhs_sync_not_cancelled()?;
            navigate_xhs_window(window, &profile_url)?;
            wait_for_xhs_window(
                window,
                "收藏专辑/文件页面",
                "(() => Boolean(document.body && document.body.innerText && window.__INITIAL_STATE__))()",
                XHS_PAGE_READY_TIMEOUT,
            )?;
            if collect_xhs_album_candidates_on_current_page(
                window,
                max_albums,
                &extraction_script,
                &mut albums,
                &mut seen,
            )? {
                return Ok(albums);
            }
            if !albums.is_empty() {
                return Ok(albums);
            }
        }
    }

    Ok(albums)
}

fn click_xhs_collection_tab<R: Runtime>(
    window: &WebviewWindow<R>,
    labels: &[&str],
) -> Result<bool, String> {
    let labels_json = serde_json::to_string(labels)
        .map_err(|error| format!("生成小红书标签脚本失败：{error}"))?;
    let script = XHS_CLICK_COLLECTION_TAB_SCRIPT.replace("__TARGET_LABELS__", &labels_json);
    let value = eval_xhs_window_value(window, &script)?;
    let clicked = value
        .get("clicked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    log::info!(
        "xhs_collection_tab_click labels={} clicked={} text={} href={}",
        labels.join("/"),
        clicked,
        value.get("text").and_then(Value::as_str).unwrap_or(""),
        value.get("href").and_then(Value::as_str).unwrap_or("")
    );
    if clicked {
        std::thread::sleep(Duration::from_millis(900));
    }
    Ok(clicked)
}

fn collect_xhs_album_candidates_on_current_page<R: Runtime>(
    window: &WebviewWindow<R>,
    max_albums: usize,
    extraction_script: &str,
    albums: &mut Vec<XhsAlbumDraft>,
    seen: &mut HashSet<String>,
) -> Result<bool, String> {
    let mut stable_attempts = 0usize;
    let mut highest_count = albums.len();
    let mut highest_scroll_height = 0i64;
    for attempt in 1..=XHS_ALBUMS_MAX_SCROLL_ATTEMPTS {
        ensure_xhs_sync_not_cancelled()?;
        let value = eval_xhs_window_value(window, extraction_script)?;
        for album in xhs_album_drafts_from_value(&value) {
            let dedupe_key = format!("{}|{}", album.source_album_id, album.name);
            if seen.insert(dedupe_key) {
                log::info!(
                    "xhs_album_candidate album_id={} name={} source_url={} note_count={:?}",
                    album.source_album_id,
                    album.name,
                    album.source_url.as_deref().unwrap_or(""),
                    album.note_count
                );
                albums.push(album);
                if albums.len() >= max_albums {
                    return Ok(true);
                }
            }
        }

        let debug = log_xhs_favorites_debug(window, attempt, albums.len());
        let scroll_height = debug_i64(debug.as_ref(), "scrollHeight");
        let at_bottom = debug_bool(debug.as_ref(), "atBottom");
        let count_grew = albums.len() > highest_count;
        let page_grew = scroll_height > highest_scroll_height + 8;
        if at_bottom && !count_grew && !page_grew {
            stable_attempts += 1;
        } else {
            stable_attempts = 0;
        }
        highest_count = highest_count.max(albums.len());
        highest_scroll_height = highest_scroll_height.max(scroll_height);
        if stable_attempts >= XHS_ALBUMS_STABLE_ATTEMPTS {
            break;
        }
        scroll_xhs_favorites_window(window)?;
        std::thread::sleep(XHS_ALBUMS_SCROLL_DELAY);
    }
    Ok(false)
}

fn collect_xhs_album_notes<R: Runtime>(
    window: &WebviewWindow<R>,
    album_url: &str,
    max_notes: usize,
) -> Result<Vec<Value>, String> {
    ensure_xhs_sync_not_cancelled()?;
    if !is_probable_xhs_album_url(album_url) {
        log::warn!("xhs_album_invalid_source_url_skipped url={album_url}");
        return Ok(Vec::new());
    }
    navigate_xhs_window(window, album_url)?;
    wait_for_xhs_window(
        window,
        "专辑详情页",
        "(() => Boolean(document.body && document.body.innerText && window.__INITIAL_STATE__))()",
        XHS_PAGE_READY_TIMEOUT,
    )?;
    let current_href = current_xhs_window_href(window).unwrap_or_default();
    if !is_probable_xhs_album_url(&current_href) || is_xhs_generic_or_error_page(&current_href) {
        log::warn!(
            "xhs_album_navigation_rejected requested_url={} current_href={}",
            album_url,
            current_href
        );
        return Ok(Vec::new());
    }

    let extraction_script = script_with_unwrap(XHS_FAVORITES_SCRIPT);
    let api_hook_available = install_xhs_favorites_api_hook(window).is_ok();
    let mut notes = Vec::new();
    let mut seen = HashSet::new();
    let mut stable_attempts = 0usize;
    let mut highest_returned = 0usize;
    let mut highest_scroll_height = 0i64;
    let page_limit = usize::min(
        XHS_FAVORITES_MAX_SCROLL_ATTEMPTS,
        usize::max(8, max_notes.div_ceil(8) + XHS_FAVORITES_STABLE_ATTEMPTS),
    );

    for attempt in 1..=page_limit {
        ensure_xhs_sync_not_cancelled()?;
        let value = eval_xhs_window_value(window, &extraction_script)?;
        let mut batch = value.as_array().cloned().unwrap_or_default();
        if api_hook_available {
            batch.extend(drain_xhs_favorites_api_notes(window, attempt));
        }
        let batch_len = batch.len();
        let before = notes.len();
        for note in batch {
            let Some(source_note_id) = xhs_note_id(&note) else {
                continue;
            };
            if seen.insert(source_note_id) {
                notes.push(note);
                if notes.len() >= max_notes {
                    return Ok(notes);
                }
            }
        }

        let debug = log_xhs_favorites_debug(window, attempt, batch_len);
        let scroll_height = debug_i64(debug.as_ref(), "scrollHeight");
        let at_bottom = debug_bool(debug.as_ref(), "atBottom");
        let returned_grew = batch_len > highest_returned;
        let page_grew = scroll_height > highest_scroll_height + 8;
        if at_bottom && notes.len() == before && !returned_grew && !page_grew {
            stable_attempts += 1;
        } else {
            stable_attempts = 0;
        }
        highest_returned = highest_returned.max(batch_len);
        highest_scroll_height = highest_scroll_height.max(scroll_height);
        if stable_attempts >= XHS_FAVORITES_STABLE_ATTEMPTS {
            break;
        }

        scroll_xhs_favorites_window(window)?;
        std::thread::sleep(XHS_FAVORITES_SCROLL_DELAY);
    }

    Ok(notes)
}

fn xhs_album_drafts_from_value(value: &Value) -> Vec<XhsAlbumDraft> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(xhs_album_draft)
        .collect()
}

fn xhs_album_draft(value: &Value) -> Option<XhsAlbumDraft> {
    let source_album_id = first_json_path_str(
        value,
        &[
            &["albumId"],
            &["album_id"],
            &["collectionId"],
            &["collection_id"],
            &["collectId"],
            &["collect_id"],
            &["favId"],
            &["fav_id"],
            &["id"],
        ],
    )?;
    let name = first_json_path_str(
        value,
        &[
            &["name"],
            &["title"],
            &["displayTitle"],
            &["display_title"],
            &["albumName"],
            &["album_name"],
            &["collectionName"],
            &["collection_name"],
        ],
    )
    .filter(|name| !name.trim().is_empty())
    .unwrap_or_else(|| "未命名专辑".to_string());
    let source_url = first_json_path_str(
        value,
        &[
            &["sourceUrl"],
            &["source_url"],
            &["url"],
            &["link"],
            &["href"],
            &["albumUrl"],
            &["album_url"],
            &["collectionUrl"],
            &["collection_url"],
        ],
    )
    .and_then(|url| normalize_xhs_album_url(&url));
    let cover_url = first_json_path_str(
        value,
        &[
            &["coverUrl"],
            &["cover_url"],
            &["cover", "url"],
            &["image", "url"],
        ],
    )
    .filter(|url| url.starts_with("http"));
    let note_count = first_json_path_i64(
        value,
        &[
            &["noteCount"],
            &["note_count"],
            &["count"],
            &["total"],
            &["itemsCount"],
            &["item_count"],
        ],
    )
    .and_then(|count| usize::try_from(count.max(0)).ok());
    Some(XhsAlbumDraft {
        source_album_id,
        name: name.chars().take(80).collect(),
        description: first_json_path_str(value, &[&["desc"], &["description"], &["intro"]])
            .unwrap_or_default()
            .chars()
            .take(240)
            .collect(),
        source_url,
        cover_url,
        note_count,
        raw_json: value.to_string(),
    })
}

fn normalize_xhs_album_url(raw_url: &str) -> Option<String> {
    let value = raw_url.trim();
    if value.is_empty() {
        return None;
    }
    let normalized = if value.starts_with("https://www.xiaohongshu.com/") {
        value.to_string()
    } else if value.starts_with("https://xiaohongshu.com/") {
        value.replacen(
            "https://xiaohongshu.com/",
            "https://www.xiaohongshu.com/",
            1,
        )
    } else if value.starts_with('/') && !value.starts_with("//") {
        format!("https://www.xiaohongshu.com{value}")
    } else {
        return None;
    };
    if is_probable_xhs_album_url(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn current_xhs_window_href<R: Runtime>(window: &WebviewWindow<R>) -> Result<String, String> {
    let value = eval_xhs_window_value(
        window,
        "(() => { try { return window.location.href || ''; } catch (_) { return ''; } })()",
    )?;
    Ok(value.as_str().unwrap_or_default().to_string())
}

fn is_xhs_generic_or_error_page(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    lower.is_empty()
        || lower.contains("beian.miit.gov.cn")
        || lower.contains("/404")
        || lower.contains("source=404")
        || lower.contains("/explore")
        || lower.contains("/discovery/item")
        || lower.contains("subtab=note")
        || lower.contains("subtab=file")
        || lower.contains("/file")
        || lower.contains("fileid")
        || lower.contains("file_id")
}

fn is_probable_xhs_album_url(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    if is_xhs_generic_or_error_page(&lower) {
        return false;
    }
    if !(lower.starts_with("https://www.xiaohongshu.com/")
        || lower.starts_with("https://xiaohongshu.com/"))
    {
        return false;
    }
    let has_detail_id = lower.contains("albumid")
        || lower.contains("album_id")
        || lower.contains("boardid")
        || lower.contains("board_id")
        || lower.contains("collectionid")
        || lower.contains("collection_id");
    let has_detail_path =
        lower.contains("/album/") || lower.contains("/board/") || lower.contains("/collection/");
    if lower.contains("/user/profile/") && !has_detail_id {
        return false;
    }
    has_detail_id || has_detail_path
}

#[allow(clippy::too_many_arguments)]
fn collect_xhs_favorites<R: Runtime>(
    app: &AppHandle,
    window: &WebviewWindow<R>,
    user_id: &str,
    sub_tab: &str,
    requested_max: Option<usize>,
    resume: bool,
    full_sync: bool,
    existing_note_ids: &HashSet<String>,
    checkpoint: Option<&XhsSyncCheckpoint>,
) -> Result<XhsFavoriteCollectResult, String> {
    let profile_url = format!(
        "https://www.xiaohongshu.com/user/profile/{user_id}?tab=fav&subTab={}",
        encode_url_query_value(sub_tab)
    );
    if resume && is_current_xhs_favorites_page(window, user_id) {
        log::info!("xhs_native_collect_resume_current_page");
    } else {
        log::info!("xhs_native_collect_navigate_start");
        navigate_xhs_window(window, &profile_url)?;
    }
    log::info!("xhs_native_collect_wait_page");
    wait_for_xhs_window(
        window,
        "收藏页面",
        "(() => Boolean(window.__INITIAL_STATE__ && window.__INITIAL_STATE__.user))()",
        XHS_PAGE_READY_TIMEOUT,
    )?;
    let remote_display_count = read_xhs_favorites_display_count(window);
    if let Some(count) = remote_display_count {
        log::info!("xhs_native_collect_display_count count={count}");
    } else {
        log::warn!("xhs_native_collect_display_count_unavailable");
    }
    let local_count_covers_remote = remote_display_count
        .filter(|count| *count > 0)
        .is_some_and(|count| existing_note_ids.len() >= count);
    if !full_sync
        && !resume
        && !existing_note_ids.is_empty()
        && remote_display_count.is_some()
        && local_count_covers_remote
        && checkpoint
            .and_then(|checkpoint| checkpoint.remote_display_count)
            .is_some_and(|previous_count| Some(previous_count) == remote_display_count)
    {
        log::info!(
            "xhs_native_collect_skip_count_unchanged count={:?}",
            remote_display_count
        );
        emit_xhs_favorites_fetch_progress(
            app,
            requested_max,
            remote_display_count,
            0,
            0,
            0,
            1,
            1,
            "收藏数量未变化，快速同步已跳过；需要校准时可点击完整同步。".to_string(),
        );
        return Ok(XhsFavoriteCollectResult {
            notes: Vec::new(),
            seen_note_ids: HashSet::new(),
            favorite_positions: Vec::new(),
            scanned: 0,
            existing_skipped: 0,
            limit_reached: false,
            reached_end: false,
            stopped_reason: "收藏数量未变化，快速同步已跳过".to_string(),
            remote_display_count,
            first_source_note_id: None,
            last_source_note_id: None,
        });
    }

    let mut notes = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_note_ids = HashSet::new();
    let mut favorite_positions = Vec::new();
    let mut scanned = 0usize;
    let mut existing_skipped = 0usize;
    let mut first_source_note_id = None;
    let mut last_source_note_id = None;
    let anchor_source_note_id = resume
        .then(|| checkpoint.and_then(|checkpoint| checkpoint.anchor_source_note_id.clone()))
        .flatten();
    let mut waiting_for_anchor = anchor_source_note_id.is_some();
    let page_limit = requested_max.map_or(XHS_FAVORITES_MAX_SCROLL_ATTEMPTS, |max_count| {
        usize::min(
            XHS_FAVORITES_MAX_SCROLL_ATTEMPTS,
            usize::max(8, max_count.div_ceil(8) + XHS_FAVORITES_STABLE_ATTEMPTS),
        )
    });
    let extraction_script = script_with_unwrap(XHS_FAVORITES_SCRIPT);
    let mut stable_attempts = 0usize;
    let mut highest_returned = 0usize;
    let mut highest_scroll_height = 0i64;
    let api_hook_available = match install_xhs_favorites_api_hook(window) {
        Ok(()) => {
            log::info!("xhs_native_collect_api_hook_installed");
            true
        }
        Err(error) => {
            log::warn!("xhs_native_collect_api_hook_install_failed {error}");
            false
        }
    };

    for scroll_attempt in 0..page_limit {
        ensure_xhs_sync_not_cancelled()?;
        let attempt = scroll_attempt + 1;
        log::info!("xhs_native_collect_extract_page attempt={attempt}");
        let value = eval_xhs_window_value(window, &extraction_script)?;
        let mut batch = value.as_array().cloned().unwrap_or_default();
        if api_hook_available {
            batch.extend(drain_xhs_favorites_api_notes(window, attempt));
        }
        let batch_len = batch.len();
        let debug = log_xhs_favorites_debug(window, attempt, batch_len);
        let before_count = notes.len();
        for note in batch {
            let Some(source_note_id) = xhs_note_id(&note) else {
                continue;
            };
            if seen_ids.insert(source_note_id.clone()) {
                scanned += 1;
                first_source_note_id.get_or_insert_with(|| source_note_id.clone());
                last_source_note_id = Some(source_note_id.clone());
                seen_note_ids.insert(source_note_id.clone());
                favorite_positions.push((source_note_id.clone(), scanned));

                if waiting_for_anchor {
                    if anchor_source_note_id.as_deref() == Some(source_note_id.as_str()) {
                        waiting_for_anchor = false;
                    } else {
                        if existing_note_ids.contains(&source_note_id) {
                            existing_skipped += 1;
                        }
                        continue;
                    }
                    continue;
                }

                if existing_note_ids.contains(&source_note_id) {
                    existing_skipped += 1;
                    if !full_sync {
                        emit_xhs_favorites_fetch_progress(
                            app,
                            requested_max,
                            remote_display_count,
                            scanned,
                            notes.len(),
                            existing_skipped,
                            attempt,
                            page_limit,
                            format!(
                                "扫描 {scanned} 条，遇到已同步收藏，快速同步停止；本次新增 {} 条。",
                                notes.len()
                            ),
                        );
                        return Ok(XhsFavoriteCollectResult {
                            notes,
                            seen_note_ids,
                            favorite_positions,
                            scanned,
                            existing_skipped,
                            limit_reached: false,
                            reached_end: false,
                            stopped_reason: "遇到已同步收藏，快速同步停止".to_string(),
                            remote_display_count,
                            first_source_note_id,
                            last_source_note_id,
                        });
                    }
                }

                notes.push(note);
                if let Some(limit) = requested_max {
                    if notes.len() < limit {
                        continue;
                    }
                    emit_xhs_favorites_fetch_progress(
                        app,
                        requested_max,
                        remote_display_count,
                        scanned,
                        notes.len(),
                        existing_skipped,
                        attempt,
                        page_limit,
                        format!(
                            "扫描 {scanned} 条，需写入 {} 条，跳过已存在 {existing_skipped} 条，达到本次读取上限。",
                            notes.len()
                        ),
                    );
                    return Ok(XhsFavoriteCollectResult {
                        notes,
                        seen_note_ids,
                        favorite_positions,
                        scanned,
                        existing_skipped,
                        limit_reached: true,
                        reached_end: false,
                        stopped_reason: "达到本次读取上限".to_string(),
                        remote_display_count,
                        first_source_note_id,
                        last_source_note_id,
                    });
                }
            }
        }

        let added_count = notes.len().saturating_sub(before_count);
        let scroll_height = debug_i64(debug.as_ref(), "scrollHeight");
        let at_bottom = debug_bool(debug.as_ref(), "atBottom");
        let returned_grew = batch_len > highest_returned;
        let page_grew = scroll_height > highest_scroll_height + 8;
        if at_bottom && added_count == 0 && !returned_grew && !page_grew {
            stable_attempts += 1;
        } else {
            stable_attempts = 0;
        }
        highest_returned = highest_returned.max(batch_len);
        highest_scroll_height = highest_scroll_height.max(scroll_height);

        let detail = match requested_max {
            Some(max_count) => format!(
                "扫描 {scanned} 条，需写入 {} / 最多 {max_count} 条，跳过已存在 {existing_skipped} 条。",
                notes.len()
            ),
            None if added_count > 0 => {
                format!(
                    "本轮新增 {added_count} 条，累计扫描 {scanned} 条，需写入 {} 条，跳过已存在 {existing_skipped} 条。",
                    notes.len()
                )
            }
            None if waiting_for_anchor => {
                let anchor = anchor_source_note_id.as_deref().unwrap_or("上次断点");
                format!("正在寻找断点 {anchor}，已扫描 {scanned} 条。")
            }
            None if !at_bottom => format!(
                "本轮没有新收藏，但还没到页面底部，继续滚动确认；已扫描 {scanned} 条。"
            ),
            None => format!(
                "本轮没有新收藏，正在确认是否已到末尾（{stable_attempts}/{XHS_FAVORITES_STABLE_ATTEMPTS}）。"
            ),
        };
        emit_xhs_favorites_fetch_progress(
            app,
            requested_max,
            remote_display_count,
            scanned,
            notes.len(),
            existing_skipped,
            attempt,
            page_limit,
            detail,
        );

        if stable_attempts >= XHS_FAVORITES_STABLE_ATTEMPTS {
            return Ok(XhsFavoriteCollectResult {
                notes,
                seen_note_ids,
                favorite_positions,
                scanned,
                existing_skipped,
                limit_reached: false,
                reached_end: !waiting_for_anchor,
                stopped_reason: "连续多次滚动没有发现新收藏".to_string(),
                remote_display_count,
                first_source_note_id,
                last_source_note_id,
            });
        }

        scroll_xhs_favorites_window(window)?;
        std::thread::sleep(XHS_FAVORITES_SCROLL_DELAY);
    }

    Ok(XhsFavoriteCollectResult {
        notes,
        seen_note_ids,
        favorite_positions,
        scanned,
        existing_skipped,
        limit_reached: true,
        reached_end: false,
        stopped_reason: format!("达到最大滚动次数 {page_limit}"),
        remote_display_count,
        first_source_note_id,
        last_source_note_id,
    })
}

fn install_xhs_favorites_api_hook<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    let value = eval_xhs_window_value(window, XHS_FAVORITES_API_HOOK_INSTALL_SCRIPT)?;
    if value
        .get("installed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err("页面没有确认安装收藏 API hook。".to_string())
    }
}

fn drain_xhs_favorites_api_notes<R: Runtime>(
    window: &WebviewWindow<R>,
    attempt: usize,
) -> Vec<Value> {
    let value = match eval_xhs_window_value(window, XHS_FAVORITES_API_HOOK_DRAIN_SCRIPT) {
        Ok(value) => value,
        Err(error) => {
            log::warn!("xhs_native_collect_api_hook_drain_failed attempt={attempt} {error}");
            return Vec::new();
        }
    };

    if let Some(errors) = value.get("errors").and_then(Value::as_array) {
        for error in errors.iter().filter_map(Value::as_str).take(5) {
            if error != "not_installed" {
                log::warn!("xhs_native_collect_api_hook_error attempt={attempt} {error}");
            }
        }
    }

    let Some(pages) = value.get("pages").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut notes = Vec::new();
    let mut page_summaries = Vec::new();
    for page in pages {
        let page_notes = page
            .get("notes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let count = page_notes.len();
        let cursor = page
            .get("cursor")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let first = page
            .get("first")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let last = page.get("last").and_then(Value::as_str).unwrap_or_default();
        page_summaries.push(format!("{count}:{first}->{last}:{cursor}"));
        notes.extend(page_notes);
    }

    if !pages.is_empty() {
        log::info!(
            "xhs_native_collect_api_hook_pages attempt={} pages={} notes={} summary={}",
            attempt,
            pages.len(),
            notes.len(),
            page_summaries.join("|")
        );
    }

    notes
}

fn read_xhs_favorites_display_count<R: Runtime>(window: &WebviewWindow<R>) -> Option<usize> {
    for _ in 0..12 {
        if let Ok(value) = eval_xhs_window_value(window, XHS_FAVORITES_DISPLAY_COUNT_SCRIPT) {
            if let Some(count) = value.as_u64().and_then(|count| usize::try_from(count).ok()) {
                if count > 0 {
                    return Some(count);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn emit_xhs_favorites_fetch_progress(
    app: &AppHandle,
    requested_max: Option<usize>,
    remote_display_count: Option<usize>,
    scanned: usize,
    to_write: usize,
    existing_skipped: usize,
    attempt: usize,
    page_limit: usize,
    detail: String,
) {
    let planned = requested_max.or(remote_display_count).unwrap_or(0);
    let (progress, indeterminate) = if let Some(max_count) = requested_max {
        (
            (48 + ((to_write * 22) / usize::max(max_count, 1)).min(22)) as u8,
            false,
        )
    } else if let Some(total) = remote_display_count.filter(|total| *total > 0) {
        (
            (48 + ((scanned * 22) / usize::max(total, 1)).min(22)) as u8,
            false,
        )
    } else {
        (
            (48 + (attempt * 22 / usize::max(page_limit, 1))).min(70) as u8,
            true,
        )
    };

    emit_xhs_sync_progress(
        app,
        XhsSyncProgress {
            phase: "fetching_favorites".to_string(),
            label: "读取收藏列表".to_string(),
            detail,
            planned,
            scanned,
            fetched: scanned,
            to_sync: Some(to_write),
            written: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            existing_skipped,
            progress,
            indeterminate,
        },
    );
}

fn scroll_xhs_favorites_window<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    window
        .eval(
            r#"(() => {
  const distance = Math.max((window.innerHeight || 700) * 1.55, 900);
  window.scrollBy(0, distance);
  window.dispatchEvent(new Event('scroll'));
  document.dispatchEvent(new Event('scroll'));
})()"#,
        )
        .map_err(|error| format!("滚动收藏页面失败：{error}"))
}

fn is_current_xhs_favorites_page<R: Runtime>(window: &WebviewWindow<R>, user_id: &str) -> bool {
    let Ok(value) = eval_xhs_window_value(
        window,
        "(() => { try { return window.location.href || ''; } catch (_) { return ''; } })()",
    ) else {
        return false;
    };

    let href = value.as_str().unwrap_or_default();
    href.contains(&format!("/user/profile/{user_id}")) && href.contains("tab=fav")
}

fn log_xhs_favorites_debug<R: Runtime>(
    window: &WebviewWindow<R>,
    attempt: usize,
    returned: usize,
) -> Option<Value> {
    match eval_xhs_window_value(window, XHS_FAVORITES_DEBUG_SCRIPT) {
        Ok(debug) => {
            let href = debug
                .get("href")
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(140)
                .collect::<String>();
            let user_key_count = debug
                .get("userKeys")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let candidate_paths = debug
                .get("candidatePaths")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .take(4)
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .unwrap_or_default();
            let link_count = debug.get("linkCount").and_then(Value::as_i64).unwrap_or(0);
            let card_count = debug.get("cardCount").and_then(Value::as_i64).unwrap_or(0);
            let state_count = debug.get("stateCount").and_then(Value::as_i64).unwrap_or(0);
            let dom_count = debug.get("domCount").and_then(Value::as_i64).unwrap_or(0);
            let scroll_y = debug_i64(Some(&debug), "scrollY");
            let scroll_height = debug_i64(Some(&debug), "scrollHeight");
            let at_bottom = debug
                .get("atBottom")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            log::info!(
                "xhs_native_collect_debug attempt={} returned={} state_count={} dom_count={} link_count={} card_count={} scroll_y={} scroll_height={} at_bottom={} user_key_count={} candidates={} href={}",
                attempt,
                returned,
                state_count,
                dom_count,
                link_count,
                card_count,
                scroll_y,
                scroll_height,
                at_bottom,
                user_key_count,
                candidate_paths,
                href
            );
            Some(debug)
        }
        Err(error) => {
            log::warn!("xhs_native_collect_debug_failed attempt={attempt} {error}");
            None
        }
    }
}

fn debug_i64(debug: Option<&Value>, key: &str) -> i64 {
    debug
        .and_then(|value| value.get(key))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn debug_bool(debug: Option<&Value>, key: &str) -> bool {
    debug
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn remote_unreturned_count(scanned: usize, remote_display_count: Option<usize>) -> Option<usize> {
    remote_display_count
        .and_then(|count| count.checked_sub(scanned))
        .filter(|count| *count > 0)
}

#[allow(clippy::too_many_arguments)]
fn sync_completion_message(
    fetched: usize,
    summary: &ImportSummary,
    scanned: usize,
    existing_skipped: usize,
    remote_missing: usize,
    limit_reached: bool,
    requested_max: Option<usize>,
    remote_display_count: Option<usize>,
    warn_remote_gap: bool,
    stopped_reason: &str,
) -> String {
    let base = format!(
        "同步完成：扫描 {scanned} 条，需写入 {fetched} 条，新增 {}，更新 {}，跳过已存在 {existing_skipped} 条，远端缺失标记 {remote_missing} 条。",
        summary.inserted, summary.updated
    );
    let remote_gap = remote_unreturned_count(scanned, remote_display_count);
    if limit_reached {
        if requested_max.is_none() {
            return format!("{base} 本轮达到滚动保护上限，可以继续下一批。");
        }
        return format!("{base} 已达到本次读取上限，可以继续同步下一批。");
    }
    if warn_remote_gap {
        if let (Some(display_count), Some(gap)) = (remote_display_count, remote_gap) {
            return format!(
                "{base} 小红书页面显示笔记 {display_count} 条，但当前 Web 收藏列表到末尾只返回 {scanned} 条；剩余 {gap} 条没有通过页面接口返回，暂不标记为已取消收藏。"
            );
        }
    }
    if stopped_reason.contains("快速同步") {
        return format!("{base} {stopped_reason}。");
    }
    if let Some(max_count) = requested_max {
        if fetched >= max_count {
            return format!("{base} 已达到本次读取上限 {max_count} 条。");
        }
    }
    base
}

fn sync_completion_with_assets(
    base: String,
    details: &BatchJobResult,
    covers: &BatchJobResult,
    media: &BatchJobResult,
) -> String {
    if details.scanned == 0 && covers.scanned == 0 && media.scanned == 0 {
        return base;
    }

    format!(
        "{base} 内容补全 {} / {} 条，轻量封面下载 {} 个，首批图片/视频下载 {} 个{}。",
        details.updated,
        details.scanned,
        covers.downloaded,
        media.downloaded,
        if details.failed + covers.failed + media.failed > 0 {
            format!(
                "，失败 {} 个",
                details.failed + covers.failed + media.failed
            )
        } else {
            String::new()
        }
    )
}

fn emit_xhs_sync_progress(app: &AppHandle, progress: XhsSyncProgress) {
    log::info!(
        "xhs_sync_progress phase={} progress={} scanned={} fetched={} written={} inserted={} updated={} skipped={} existing_skipped={}",
        progress.phase,
        progress.progress,
        progress.scanned,
        progress.fetched,
        progress.written,
        progress.inserted,
        progress.updated,
        progress.skipped,
        progress.existing_skipped
    );
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = app.emit_to("main", "xhs-sync-progress", progress) {
            log::warn!("xhs_sync_progress_emit_failed {error}");
        }
    });
}

fn ensure_xhs_sync_not_cancelled() -> Result<(), String> {
    if xhs_sync_cancel_requested() {
        Err("同步已终止。".to_string())
    } else {
        Ok(())
    }
}

fn batch_progress_percent(scanned: usize, planned: usize) -> u8 {
    if planned == 0 {
        return 100;
    }
    (((scanned * 100) / planned).min(100)) as u8
}

fn emit_batch_progress(app: &AppHandle, event_name: &'static str, progress: BatchJobProgress) {
    log::info!(
        "batch_progress event={} phase={} progress={} planned={} scanned={} updated={} downloaded={} failed={} skipped={}",
        event_name,
        progress.phase,
        progress.progress,
        progress.planned,
        progress.scanned,
        progress.updated,
        progress.downloaded,
        progress.failed,
        progress.skipped
    );
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = app.emit_to("main", event_name, progress) {
            log::warn!("batch_progress_emit_failed event={event_name} {error}");
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

fn read_xhs_detail_targets(
    conn: &Connection,
    limit: Option<usize>,
) -> rusqlite::Result<Vec<NoteFetchTarget>> {
    let sql = format!(
        "SELECT n.id, n.source_note_id, n.source_url
         FROM notes n
         WHERE n.source = 'xhs'
           AND n.remote_status = 'available'
           AND COALESCE(n.source_url, '') <> ''
           AND (
             COALESCE(n.content, '') = ''
             OR NOT EXISTS (
               SELECT 1 FROM note_tags nt
               WHERE nt.note_id = n.id
             )
             OR NOT EXISTS (
               SELECT 1 FROM media_assets m
               WHERE m.note_id = n.id AND m.media_type IN ('image', 'video')
             )
           )
         ORDER BY
           CASE WHEN n.collected_at IS NULL OR n.collected_at = '' THEN 1 ELSE 0 END ASC,
           datetime(n.collected_at) DESC,
           COALESCE(n.favorite_order, 999999999) ASC,
           datetime(n.last_seen_at) DESC
         {}",
        if limit.is_some() { "LIMIT ?1" } else { "" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let read_row = |row: &rusqlite::Row<'_>| {
        Ok(NoteFetchTarget {
            id: row.get(0)?,
            source_note_id: row.get(1)?,
            source_url: row.get(2)?,
        })
    };
    if let Some(limit) = limit {
        let rows = stmt.query_map(params![limit as i64], read_row)?;
        rows.collect()
    } else {
        let rows = stmt.query_map([], read_row)?;
        rows.collect()
    }
}

fn read_xhs_detail_targets_by_ids(
    conn: &Connection,
    note_ids: &[String],
) -> rusqlite::Result<Vec<NoteFetchTarget>> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT n.id, n.source_note_id, n.source_url
         FROM notes n
         WHERE n.id = ?1
           AND n.source = 'xhs'
           AND n.remote_status = 'available'
           AND COALESCE(n.source_url, '') <> ''
           AND (
             COALESCE(n.content, '') = ''
             OR COALESCE(n.author_name, '') = ''
             OR NOT EXISTS (
               SELECT 1 FROM note_tags nt
               WHERE nt.note_id = n.id
             )
             OR NOT EXISTS (
               SELECT 1 FROM media_assets m
               WHERE m.note_id = n.id AND m.media_type IN ('image', 'video')
             )
           )",
    )?;
    for note_id in note_ids {
        if !seen.insert(note_id.as_str()) {
            continue;
        }
        if let Some(target) = stmt
            .query_row(params![note_id], |row| {
                Ok(NoteFetchTarget {
                    id: row.get(0)?,
                    source_note_id: row.get(1)?,
                    source_url: row.get(2)?,
                })
            })
            .optional()?
        {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn read_media_download_targets(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<MediaDownloadTarget>> {
    let mut stmt = conn.prepare(
        "SELECT
            m.id,
            m.note_id,
            m.source_asset_id,
            n.source_note_id,
            m.media_type,
            m.original_url,
            m.mime_type
         FROM media_assets m
         INNER JOIN notes n ON n.id = m.note_id
         WHERE COALESCE(m.original_url, '') <> ''
           AND COALESCE(m.download_status, 'not_downloaded') <> 'downloaded'
         ORDER BY
           CASE m.media_type
             WHEN 'cover' THEN 0
             WHEN 'image' THEN 1
             WHEN 'video' THEN 2
             WHEN 'file' THEN 3
             ELSE 4
           END,
           CASE WHEN n.collected_at IS NULL OR n.collected_at = '' THEN 1 ELSE 0 END ASC,
           datetime(n.collected_at) DESC,
           COALESCE(n.favorite_order, 999999999) ASC,
           m.created_at ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(MediaDownloadTarget {
            id: row.get(0)?,
            note_id: row.get(1)?,
            source_asset_id: row.get(2)?,
            note_source_note_id: row.get(3)?,
            media_type: row.get(4)?,
            original_url: row.get(5)?,
            mime_type: row.get(6)?,
        })
    })?;
    rows.collect()
}

fn read_media_download_targets_filtered(
    conn: &Connection,
    limit: usize,
    media_types: &[&str],
    note_ids: Option<&HashSet<String>>,
) -> rusqlite::Result<Vec<MediaDownloadTarget>> {
    let mut stmt = conn.prepare(
        "SELECT
            m.id,
            m.note_id,
            m.source_asset_id,
            n.source_note_id,
            m.media_type,
            m.original_url,
            m.mime_type
         FROM media_assets m
         INNER JOIN notes n ON n.id = m.note_id
         WHERE COALESCE(m.original_url, '') <> ''
           AND COALESCE(m.download_status, 'not_downloaded') <> 'downloaded'
         ORDER BY
           CASE WHEN n.collected_at IS NULL OR n.collected_at = '' THEN 1 ELSE 0 END ASC,
           datetime(n.collected_at) DESC,
           COALESCE(n.favorite_order, 999999999) ASC,
           CASE m.media_type
             WHEN 'cover' THEN 0
             WHEN 'image' THEN 1
             WHEN 'video' THEN 2
             WHEN 'file' THEN 3
             ELSE 4
           END,
           m.created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MediaDownloadTarget {
            id: row.get(0)?,
            note_id: row.get(1)?,
            source_asset_id: row.get(2)?,
            note_source_note_id: row.get(3)?,
            media_type: row.get(4)?,
            original_url: row.get(5)?,
            mime_type: row.get(6)?,
        })
    })?;

    let mut targets = Vec::new();
    for row in rows {
        let target = row?;
        if !media_types
            .iter()
            .any(|media_type| *media_type == target.media_type)
        {
            continue;
        }
        if let Some(note_ids) = note_ids {
            if !note_ids.contains(&target.note_id) {
                continue;
            }
        }
        targets.push(target);
        if targets.len() >= limit {
            break;
        }
    }
    Ok(targets)
}

fn read_media_download_target_by_id(
    conn: &Connection,
    asset_id: &str,
) -> rusqlite::Result<Option<MediaDownloadTarget>> {
    conn.query_row(
        "SELECT
            m.id,
            m.note_id,
            m.source_asset_id,
            n.source_note_id,
            m.media_type,
            m.original_url,
            m.mime_type
         FROM media_assets m
         INNER JOIN notes n ON n.id = m.note_id
         WHERE m.id = ?1
           AND COALESCE(m.original_url, '') <> ''
           AND COALESCE(m.download_status, 'not_downloaded') <> 'downloaded'",
        params![asset_id],
        |row| {
            Ok(MediaDownloadTarget {
                id: row.get(0)?,
                note_id: row.get(1)?,
                source_asset_id: row.get(2)?,
                note_source_note_id: row.get(3)?,
                media_type: row.get(4)?,
                original_url: row.get(5)?,
                mime_type: row.get(6)?,
            })
        },
    )
    .optional()
}

fn read_media_download_targets_by_note_id(
    conn: &Connection,
    note_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<MediaDownloadTarget>> {
    let mut stmt = conn.prepare(
        "SELECT
            m.id,
            m.note_id,
            m.source_asset_id,
            n.source_note_id,
            m.media_type,
            m.original_url,
            m.mime_type
         FROM media_assets m
         INNER JOIN notes n ON n.id = m.note_id
         WHERE m.note_id = ?1
           AND COALESCE(m.original_url, '') <> ''
           AND COALESCE(m.download_status, 'not_downloaded') <> 'downloaded'
         ORDER BY
           CASE m.media_type
             WHEN 'cover' THEN 0
             WHEN 'image' THEN 1
             WHEN 'video' THEN 2
             WHEN 'file' THEN 3
             ELSE 4
           END,
           m.created_at ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![note_id, limit as i64], |row| {
        Ok(MediaDownloadTarget {
            id: row.get(0)?,
            note_id: row.get(1)?,
            source_asset_id: row.get(2)?,
            note_source_note_id: row.get(3)?,
            media_type: row.get(4)?,
            original_url: row.get(5)?,
            mime_type: row.get(6)?,
        })
    })?;
    rows.collect()
}

async fn fetch_xhs_note_detail_resilient(
    app: &AppHandle,
    client: &Client,
    cookie_header: &str,
    target: &NoteFetchTarget,
) -> Result<Value, XhsDetailFetchError> {
    match fetch_xhs_note_detail(client, cookie_header, target).await {
        Ok(detail) => Ok(detail),
        Err(XhsDetailFetchError::Gone(status_code)) => Err(XhsDetailFetchError::Gone(status_code)),
        Err(XhsDetailFetchError::NeedsVerification(url)) => {
            let _ = show_xhs_verification_window(app, cookie_header, target);
            Err(XhsDetailFetchError::NeedsVerification(url))
        }
        Err(XhsDetailFetchError::Other(error)) => {
            log::warn!(
                "xhs_detail_http_fetch_failed_try_tauri note_id={} error={}",
                target.source_note_id,
                error
            );
            fetch_xhs_note_detail_from_tauri_window(app, cookie_header, target).map_err(
                |fallback_error| {
                    if is_xhs_verification_text(&fallback_error) {
                        let _ = show_xhs_verification_window(app, cookie_header, target);
                        return XhsDetailFetchError::NeedsVerification(fallback_error);
                    }
                    XhsDetailFetchError::Other(format!(
                        "{error}；Tauri 页面兜底也失败：{fallback_error}"
                    ))
                },
            )
        }
    }
}

fn show_xhs_verification_window(
    app: &AppHandle,
    cookie_header: &str,
    target: &NoteFetchTarget,
) -> Result<(), String> {
    let detail_url = normalize_xhs_note_url(&target.source_url, &target.source_note_id)
        .unwrap_or_else(|| {
            format!(
                "https://www.xiaohongshu.com/explore/{}",
                target.source_note_id
            )
        });
    let window = if let Some(window) = app.get_webview_window(XHS_DETAIL_WINDOW_LABEL) {
        window
    } else {
        let login_url = Url::parse(XHS_LOGIN_URL).map_err(|error| error.to_string())?;
        WebviewWindowBuilder::new(
            app,
            XHS_DETAIL_WINDOW_LABEL,
            WebviewUrl::External(login_url),
        )
        .title("小红书验证 - XHS Collection")
        .inner_size(980.0, 760.0)
        .min_inner_size(760.0, 560.0)
        .visible(true)
        .focused(true)
        .build()
        .map_err(|error| format!("打开小红书验证窗口失败：{error}"))?
    };
    let _ = inject_xhs_cookie_header(&window, cookie_header);
    let _ = navigate_xhs_window(&window, &detail_url);
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

fn fetch_xhs_note_detail_from_tauri_window(
    app: &AppHandle,
    cookie_header: &str,
    target: &NoteFetchTarget,
) -> Result<Value, String> {
    let detail_url = normalize_xhs_note_url(&target.source_url, &target.source_note_id)
        .unwrap_or_else(|| {
            format!(
                "https://www.xiaohongshu.com/explore/{}",
                target.source_note_id
            )
        });
    let window = get_or_create_xhs_detail_window(app, cookie_header)?;
    log::info!(
        "xhs_detail_tauri_fetch_start note_id={} url={}",
        target.source_note_id,
        detail_url
    );
    navigate_xhs_window(&window, &detail_url)?;
    let _ = wait_for_xhs_window(
        &window,
        "笔记详情",
        "(() => document.readyState === 'complete' || Boolean(window.__INITIAL_STATE__) || Boolean(document.body && document.body.innerText))()",
        XHS_PAGE_READY_TIMEOUT,
    );

    let mut last_error = String::new();
    for attempt in 1..=8 {
        ensure_xhs_sync_not_cancelled()?;
        match extract_xhs_note_detail_from_window(&window, &target.source_note_id) {
            Ok(detail) => {
                log::info!(
                    "xhs_detail_tauri_fetch_done note_id={} attempt={}",
                    target.source_note_id,
                    attempt
                );
                return Ok(detail);
            }
            Err(error) => {
                last_error = error;
                std::thread::sleep(Duration::from_millis(650));
            }
        }
    }

    Err(if last_error.is_empty() {
        "Tauri 页面没有返回可用笔记详情。".to_string()
    } else {
        last_error
    })
}

fn get_or_create_xhs_detail_window(
    app: &AppHandle,
    cookie_header: &str,
) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(XHS_DETAIL_WINDOW_LABEL) {
        let _ = inject_xhs_cookie_header(&window, cookie_header);
        return Ok(window);
    }

    let login_url = Url::parse(XHS_LOGIN_URL).map_err(|error| error.to_string())?;
    let window = WebviewWindowBuilder::new(
        app,
        XHS_DETAIL_WINDOW_LABEL,
        WebviewUrl::External(login_url),
    )
    .title("小红书详情补全 - XHS Collection")
    .inner_size(980.0, 760.0)
    .visible(false)
    .focused(false)
    .build()
    .map_err(|error| format!("创建小红书详情补全窗口失败：{error}"))?;
    std::thread::sleep(Duration::from_millis(800));
    inject_xhs_cookie_header(&window, cookie_header)?;
    Ok(window)
}

fn extract_xhs_note_detail_from_window<R: Runtime>(
    window: &WebviewWindow<R>,
    source_note_id: &str,
) -> Result<Value, String> {
    let note_id_json = serde_json::to_string(source_note_id).unwrap_or_else(|_| "\"\"".to_string());
    let script = script_with_unwrap(&format!(
        r#"
(() => {{
__UNWRAP_JS__
  const sourceNoteId = {note_id_json};
  const state = unwrap(window.__INITIAL_STATE__, 0);
  if (!state || typeof state !== 'object') {{
    return {{ ok: false, error: 'missing_initial_state', href: window.location.href || '', title: document.title || '' }};
  }}

  function get(obj, path) {{
    let cur = obj;
    for (const key of path) {{
      if (cur === null || cur === undefined) return '';
      cur = cur[key];
    }}
    return cur === null || cur === undefined ? '' : String(cur).trim();
  }}

  function first(obj, paths) {{
    for (const path of paths) {{
      const value = get(obj, path);
      if (value) return value;
    }}
    return '';
  }}

  function pick(entry) {{
    const data = unwrap(entry, 0);
    if (!data || typeof data !== 'object') return null;
    return unwrap(data.note || data.noteCard || data.note_card || data, 0);
  }}

  function noteIdOf(item) {{
    return first(item, [
      ['noteId'], ['note_id'], ['id'],
      ['note', 'noteId'], ['note', 'note_id'], ['note', 'id'],
      ['noteCard', 'noteId'], ['noteCard', 'note_id'], ['noteCard', 'id'],
      ['note_card', 'noteId'], ['note_card', 'note_id'], ['note_card', 'id']
    ]);
  }}

  const detailMaps = [
    state.note && state.note.noteDetailMap,
    state.noteDetailMap,
    state.note && state.note.detailMap,
    state.detailMap
  ].filter(Boolean).map(item => unwrap(item, 0)).filter(item => item && typeof item === 'object');

  let firstDetail = null;
  for (const map of detailMaps) {{
    const direct = pick(map[sourceNoteId]);
    if (direct && noteIdOf(direct)) return {{ ok: true, note: direct }};
    for (const value of Object.values(map)) {{
      const note = pick(value);
      if (!note || typeof note !== 'object') continue;
      if (!firstDetail && noteIdOf(note)) firstDetail = note;
      if (noteIdOf(note) === sourceNoteId) return {{ ok: true, note }};
    }}
  }}

  const seen = new WeakSet();
  function scan(obj, depth) {{
    if (!obj || depth > 8) return null;
    const data = unwrap(obj, 0);
    if (!data || typeof data !== 'object') return null;
    if (seen.has(data)) return null;
    seen.add(data);
    const note = pick(data);
    if (note && typeof note === 'object') {{
      const id = noteIdOf(note);
      if (id === sourceNoteId) return note;
      if (!firstDetail && id) firstDetail = note;
    }}
    const keys = Object.keys(data).sort((a, b) => {{
      const rank = key => /note|detail|card|feed|item/i.test(key) ? 0 : 1;
      return rank(a) - rank(b);
    }});
    for (const key of keys) {{
      if (key === 'dep' || key.startsWith('__')) continue;
      const found = scan(data[key], depth + 1);
      if (found) return found;
    }}
    return null;
  }}

  const found = scan(state, 0);
  if (found) return {{ ok: true, note: found }};
  if (firstDetail) return {{ ok: true, note: firstDetail }};

  return {{
    ok: false,
    error: 'note_detail_not_found',
    href: window.location.href || '',
    title: document.title || '',
    stateKeys: Object.keys(state).slice(0, 40)
  }};
}})()
"#
    ));
    let value = eval_xhs_window_value(window, &script)?;
    if value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return value
            .get("note")
            .cloned()
            .ok_or_else(|| "Tauri 页面返回成功但没有 note 字段。".to_string());
    }

    let error = value
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let href = value.get("href").and_then(Value::as_str).unwrap_or("");
    Err(format!("Tauri 页面没有找到笔记详情：{error} {href}"))
}

async fn fetch_xhs_note_detail(
    client: &Client,
    cookie_header: &str,
    target: &NoteFetchTarget,
) -> Result<Value, XhsDetailFetchError> {
    let detail_url = normalize_xhs_note_url(&target.source_url, &target.source_note_id)
        .unwrap_or_else(|| {
            format!(
                "https://www.xiaohongshu.com/explore/{}",
                target.source_note_id
            )
        });
    log::info!(
        "xhs_detail_fetch_start note_id={} url={}",
        target.source_note_id,
        detail_url
    );
    let response = client
        .get(&detail_url)
        .header(USER_AGENT, XHS_WEB_USER_AGENT)
        .header(
            ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.7")
        .header(REFERER, "https://www.xiaohongshu.com/")
        .header(COOKIE, cookie_header)
        .send()
        .await
        .map_err(|error| XhsDetailFetchError::Other(format!("请求详情页失败：{error}")))?;

    let status = response.status();
    let final_url = response.url().to_string();
    if is_xhs_verification_url(&final_url) {
        return Err(XhsDetailFetchError::NeedsVerification(final_url));
    }
    if matches!(status.as_u16(), 404 | 410) {
        return Err(XhsDetailFetchError::Gone(status.as_u16()));
    }
    if !status.is_success() {
        return Err(XhsDetailFetchError::Other(format!(
            "详情页返回 HTTP {}",
            status.as_u16()
        )));
    }

    let html = response
        .text()
        .await
        .map_err(|error| XhsDetailFetchError::Other(format!("读取详情页失败：{error}")))?;
    extract_xhs_note_detail_from_html(&html, &target.source_note_id)
        .map_err(XhsDetailFetchError::Other)
}

fn is_xhs_verification_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("/website-login/captcha")
        || lower.contains("verifytype=")
        || lower.contains("verifyuuid=")
}

fn is_xhs_verification_text(text: &str) -> bool {
    is_xhs_verification_url(text)
        || text.contains("验证码")
        || text.to_ascii_lowercase().contains("captcha")
}

fn extract_xhs_note_detail_from_html(html: &str, source_note_id: &str) -> Result<Value, String> {
    let marker_start = html
        .find("window.__INITIAL_STATE__")
        .ok_or_else(|| "详情页没有找到小红书初始状态。".to_string())?;
    let after_marker = &html[marker_start..];
    let object_start = after_marker
        .find('{')
        .map(|offset| marker_start + offset)
        .ok_or_else(|| "详情页初始状态缺少 JSON 对象。".to_string())?;
    let object_end = find_balanced_object_end(html, object_start)
        .ok_or_else(|| "详情页初始状态 JSON 不完整。".to_string())?;
    let raw_json = &html[object_start..=object_end];
    let state_json = sanitize_js_json(raw_json);
    let state: Value = serde_json::from_str(&state_json)
        .map_err(|error| format!("解析小红书详情 JSON 失败：{error}"))?;

    let detail_map = state
        .get("note")
        .and_then(|note| note.get("noteDetailMap"))
        .or_else(|| state.get("noteDetailMap"))
        .and_then(Value::as_object)
        .ok_or_else(|| "详情页没有找到 noteDetailMap。".to_string())?;

    if let Some(note) = detail_map
        .get(source_note_id)
        .and_then(|entry| entry.get("note").or(Some(entry)))
    {
        return Ok(note.clone());
    }

    detail_map
        .values()
        .find_map(|entry| entry.get("note").or(Some(entry)).cloned())
        .ok_or_else(|| "详情页 noteDetailMap 里没有可用笔记。".to_string())
}

fn find_balanced_object_end(input: &str, start: usize) -> Option<usize> {
    let mut depth = 0i64;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in input[start..].char_indices() {
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
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn sanitize_js_json(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < raw.len() {
        let remaining = &raw[index..];
        let Some(ch) = remaining.chars().next() else {
            break;
        };

        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += ch.len_utf8();
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            index += ch.len_utf8();
            continue;
        }

        if remaining.starts_with("undefined") {
            output.push_str("null");
            index += "undefined".len();
            continue;
        }

        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

fn upsert_xhs_note_detail(
    conn: &Connection,
    note_id: &str,
    source_note_id: &str,
    detail: &Value,
) -> rusqlite::Result<()> {
    let title = first_json_path_str(
        detail,
        &[
            &["title"],
            &["displayTitle"],
            &["display_title"],
            &["note", "title"],
        ],
    )
    .unwrap_or_default();
    let content = xhs_note_content(detail).unwrap_or_default();
    let excerpt = make_excerpt(&content, &title);
    let author_id = first_json_path_str(
        detail,
        &[
            &["user", "userId"],
            &["user", "user_id"],
            &["user", "id"],
            &["author", "userId"],
            &["author", "id"],
        ],
    );
    let author_name = first_json_path_str(
        detail,
        &[
            &["user", "nickname"],
            &["user", "nickName"],
            &["user", "nick_name"],
            &["author", "nickname"],
        ],
    )
    .unwrap_or_default();
    let cover_url = xhs_cover_url(detail);
    let note_type = xhs_note_type(detail);
    let published_at = xhs_published_at(detail);
    let remote_updated_at = xhs_remote_updated_at(detail);
    let raw_json = detail.to_string();

    conn.execute(
        "UPDATE notes
         SET title = CASE WHEN ?1 <> '' THEN ?1 ELSE title END,
             excerpt = CASE WHEN ?2 <> '' THEN ?2 ELSE excerpt END,
             content = CASE WHEN ?3 <> '' THEN ?3 ELSE content END,
             author_id = COALESCE(?4, author_id),
             author_name = CASE WHEN ?5 <> '' THEN ?5 ELSE author_name END,
             cover_url = COALESCE(?6, cover_url),
             note_type = CASE WHEN ?7 <> 'unknown' THEN ?7 ELSE note_type END,
             published_at = COALESCE(?8, published_at),
             remote_updated_at = COALESCE(?9, remote_updated_at),
             raw_json = ?10,
             last_seen_at = CURRENT_TIMESTAMP,
             last_synced_at = CURRENT_TIMESTAMP,
             remote_missing_at = NULL,
             remote_status = 'available',
             unavailable_reason = NULL,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?11",
        params![
            title,
            excerpt,
            content,
            author_id,
            author_name,
            cover_url,
            note_type,
            published_at,
            remote_updated_at,
            raw_json,
            note_id
        ],
    )?;

    for tag_name in xhs_detail_tags(detail, &title, &content) {
        let tag_id = upsert_tag_with_kind(conn, &tag_name, "topic")?;
        conn.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_id, tag_id],
        )?;
    }

    for asset in extract_xhs_media_assets(source_note_id, detail) {
        upsert_media_asset(conn, note_id, asset)?;
    }

    Ok(())
}

fn mark_xhs_note_unavailable(
    conn: &Connection,
    note_id: &str,
    reason: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE notes
         SET remote_missing_at = COALESCE(remote_missing_at, CURRENT_TIMESTAMP),
             remote_status = 'deleted',
             unavailable_reason = ?1,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?2",
        params![reason, note_id],
    )?;
    Ok(())
}

fn xhs_note_content(value: &Value) -> Option<String> {
    first_json_path_str(
        value,
        &[
            &["desc"],
            &["description"],
            &["content"],
            &["note", "desc"],
            &["note", "content"],
        ],
    )
}

fn xhs_remote_updated_at(value: &Value) -> Option<String> {
    xhs_time_at(
        value,
        &[
            &["lastUpdateTime"],
            &["last_update_time"],
            &["updateTime"],
            &["update_time"],
        ],
    )
}

fn make_excerpt(content: &str, fallback: &str) -> String {
    let compact = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let source = if compact.is_empty() {
        fallback.trim()
    } else {
        compact.as_str()
    };
    let mut chars = source.chars();
    let excerpt = chars.by_ref().take(160).collect::<String>();
    if chars.next().is_some() {
        format!("{excerpt}...")
    } else {
        excerpt
    }
}

fn upsert_xhs_text_tags(conn: &Connection, note_id: &str, texts: &[&str]) -> rusqlite::Result<()> {
    for tag_name in xhs_text_tags(texts) {
        let tag_id = upsert_tag_with_kind(conn, &tag_name, "topic")?;
        conn.execute(
            "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?1, ?2)",
            params![note_id, tag_id],
        )?;
    }
    Ok(())
}

fn xhs_detail_tags(value: &Value, title: &str, content: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for key in ["tagList", "tag_list", "hashTagList", "hash_tag_list"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                let name = match item {
                    Value::String(text) => Some(text.trim().trim_start_matches('#').to_string()),
                    _ => first_json_path_str(
                        item,
                        &[&["name"], &["tagName"], &["tag_name"], &["title"], &["id"]],
                    )
                    .map(|text| text.trim().trim_start_matches('#').to_string()),
                };
                if let Some(name) = name {
                    if !name.is_empty() && !is_noise_tag_name(&name) && seen.insert(name.clone()) {
                        tags.push(name);
                    }
                }
            }
        }
    }
    for name in xhs_text_tags(&[title, content]) {
        if seen.insert(name.clone()) {
            tags.push(name);
        }
    }
    tags
}

fn xhs_text_tags(texts: &[&str]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for text in texts {
        let mut offset = 0usize;
        while offset < text.len() {
            let Some((relative_start, marker)) = text[offset..]
                .char_indices()
                .find(|(_, ch)| matches!(*ch, '#' | '＃'))
            else {
                break;
            };
            let start = offset + relative_start + marker.len_utf8();
            let mut end = text.len();
            for (relative_end, ch) in text[start..].char_indices() {
                if matches!(ch, '#' | '＃' | '\n' | '\r') {
                    end = start + relative_end;
                    break;
                }
            }
            if let Some(tag) = clean_xhs_text_tag(&text[start..end]) {
                let key = tag.to_lowercase();
                if seen.insert(key) {
                    tags.push(tag);
                }
            }
            offset = if end > start { end } else { start };
        }
    }
    tags
}

fn clean_xhs_text_tag(raw: &str) -> Option<String> {
    let had_topic_marker = raw.contains("[话题]") || raw.contains("[topic]");
    let mut tag = raw
        .trim()
        .trim_end_matches("[话题]")
        .trim_end_matches("[topic]")
        .trim()
        .to_string();
    if !had_topic_marker {
        tag = tag
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
    }
    let tag = tag
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '#' | '＃'
                        | '['
                        | ']'
                        | '【'
                        | '】'
                        | '('
                        | ')'
                        | '（'
                        | '）'
                        | ','
                        | '，'
                        | '.'
                        | '。'
                        | ':'
                        | '：'
                        | ';'
                        | '；'
                        | '!'
                        | '！'
                        | '?'
                        | '？'
                )
        })
        .trim()
        .to_string();
    if tag.is_empty() || tag.chars().count() > 64 || is_noise_tag_name(&tag) {
        return None;
    }
    Some(tag)
}

fn extract_xhs_media_assets(source_note_id: &str, detail: &Value) -> Vec<ExtractedMediaAsset> {
    let mut assets = Vec::new();
    if let Some(cover_url) = xhs_cover_url(detail) {
        assets.push(ExtractedMediaAsset {
            source_asset_id: format!("{source_note_id}:cover"),
            media_type: "cover".to_string(),
            original_url: cover_url.clone(),
            mime_type: infer_mime_type("cover", &cover_url, None),
            width: None,
            height: None,
            duration_ms: None,
            size_bytes: None,
        });
    }

    for key in ["imageList", "image_list", "images"] {
        if let Some(items) = detail.get(key).and_then(Value::as_array) {
            for (index, item) in items.iter().enumerate() {
                let Some(url) = xhs_image_url(item) else {
                    continue;
                };
                assets.push(ExtractedMediaAsset {
                    source_asset_id: format!("{}:image:{}", source_note_id, index + 1),
                    media_type: "image".to_string(),
                    original_url: url.clone(),
                    mime_type: infer_mime_type("image", &url, None),
                    width: first_json_path_i64(item, &[&["width"], &["w"]]),
                    height: first_json_path_i64(item, &[&["height"], &["h"]]),
                    duration_ms: None,
                    size_bytes: None,
                });
            }
            break;
        }
    }

    if let Some(video) = extract_xhs_video_asset(source_note_id, detail) {
        assets.push(video);
    }
    assets.extend(extract_xhs_file_assets(source_note_id, detail));

    assets
}

fn xhs_image_url(value: &Value) -> Option<String> {
    first_json_path_str(
        value,
        &[
            &["urlDefault"],
            &["url_default"],
            &["urlPre"],
            &["url_pre"],
            &["url"],
            &["infoList", "0", "url"],
            &["info_list", "0", "url"],
            &["livePhoto", "imageUrl"],
            &["live_photo", "image_url"],
        ],
    )
    .filter(|url| url.starts_with("http"))
}

fn extract_xhs_video_asset(source_note_id: &str, detail: &Value) -> Option<ExtractedMediaAsset> {
    let mut roots = Vec::new();
    if let Some(media_v2_text) = first_json_path_str(
        detail,
        &[
            &["video", "mediaV2"],
            &["video", "media_v2"],
            &["mediaV2"],
            &["media_v2"],
        ],
    ) {
        if media_v2_text.trim_start().starts_with('{') {
            if let Ok(value) = serde_json::from_str::<Value>(&media_v2_text) {
                roots.push(value);
            }
        }
    }
    if let Some(video) = detail.get("video") {
        roots.push(video.clone());
    }

    let mut candidates = Vec::new();
    for root in &roots {
        collect_video_stream_candidates(root, "", &mut candidates);
    }
    let best = choose_video_stream(candidates)?;

    Some(ExtractedMediaAsset {
        source_asset_id: format!("{source_note_id}:video:main"),
        media_type: "video".to_string(),
        original_url: best.url,
        mime_type: Some("video/mp4".to_string()),
        width: best.width,
        height: best.height,
        duration_ms: best.duration_ms,
        size_bytes: best.size_bytes,
    })
}

fn extract_xhs_file_assets(source_note_id: &str, detail: &Value) -> Vec<ExtractedMediaAsset> {
    let mut urls = Vec::new();
    collect_file_urls(detail, &mut urls, &mut HashSet::new(), 0);
    urls.into_iter()
        .take(24)
        .enumerate()
        .map(|(index, url)| ExtractedMediaAsset {
            source_asset_id: format!("{}:file:{}", source_note_id, index + 1),
            media_type: "file".to_string(),
            original_url: url.clone(),
            mime_type: infer_mime_type("file", &url, None),
            width: None,
            height: None,
            duration_ms: None,
            size_bytes: None,
        })
        .collect()
}

fn collect_file_urls(
    value: &Value,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
    depth: usize,
) {
    if depth > 8 || out.len() >= 24 {
        return;
    }
    match value {
        Value::String(text) => {
            let candidate = text.trim();
            if is_downloadable_file_url(candidate) && seen.insert(candidate.to_string()) {
                out.push(candidate.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_file_urls(item, out, seen, depth + 1);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_file_urls(item, out, seen, depth + 1);
            }
        }
        _ => {}
    }
}

fn is_downloadable_file_url(value: &str) -> bool {
    if !value.starts_with("http") {
        return false;
    }
    let lower = value.to_lowercase();
    [
        ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".zip", ".rar", ".7z", ".txt",
        ".csv", ".md",
    ]
    .iter()
    .any(|extension| lower.contains(extension))
}

fn collect_video_stream_candidates(
    value: &Value,
    codec_hint: &str,
    out: &mut Vec<VideoStreamCandidate>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_video_stream_candidates(item, codec_hint, out);
            }
        }
        Value::Object(object) => {
            let codec = first_json_path_str(
                value,
                &[
                    &["codec"],
                    &["codecType"],
                    &["codec_type"],
                    &["videoCodec"],
                    &["video_codec"],
                ],
            )
            .unwrap_or_else(|| codec_hint.to_string());
            if let Some(url) = first_json_path_str(
                value,
                &[
                    &["master_url"],
                    &["masterUrl"],
                    &["main_url"],
                    &["mainUrl"],
                    &["url"],
                    &["backup_urls", "0"],
                    &["backupUrls", "0"],
                ],
            )
            .filter(|url| url.starts_with("http"))
            {
                out.push(VideoStreamCandidate {
                    url,
                    codec: codec.clone(),
                    width: first_json_path_i64(value, &[&["width"], &["w"]]),
                    height: first_json_path_i64(value, &[&["height"], &["h"]]),
                    duration_ms: first_json_path_i64(
                        value,
                        &[&["durationMs"], &["duration_ms"], &["duration"]],
                    )
                    .map(normalize_duration_ms),
                    size_bytes: first_json_path_i64(
                        value,
                        &[&["size"], &["sizeBytes"], &["size_bytes"]],
                    ),
                });
            }

            for (key, child) in object {
                let lower_key = key.to_lowercase();
                let next_codec = if lower_key.contains("h264")
                    || lower_key.contains("h265")
                    || lower_key.contains("h266")
                    || lower_key.contains("av1")
                {
                    key.as_str()
                } else {
                    codec_hint
                };
                collect_video_stream_candidates(child, next_codec, out);
            }
        }
        _ => {}
    }
}

fn choose_video_stream(candidates: Vec<VideoStreamCandidate>) -> Option<VideoStreamCandidate> {
    candidates.into_iter().max_by_key(|candidate| {
        let codec = candidate.codec.to_lowercase();
        let compatibility = if codec.contains("h264") { 1 } else { 0 };
        let pixels = candidate.width.unwrap_or(0).max(0) * candidate.height.unwrap_or(0).max(0);
        let size = candidate.size_bytes.unwrap_or(0).max(0);
        (compatibility, pixels, size)
    })
}

fn upsert_media_asset(
    conn: &Connection,
    note_id: &str,
    asset: ExtractedMediaAsset,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO media_assets (
            id, note_id, source, source_asset_id, media_type, original_url,
            storage_root_id, mime_type, width, height, duration_ms, size_bytes, download_status
         )
         VALUES (?1, ?2, 'xhs', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'not_downloaded')
         ON CONFLICT(note_id, source_asset_id) DO UPDATE SET
            media_type = excluded.media_type,
            original_url = excluded.original_url,
            storage_root_id = excluded.storage_root_id,
            mime_type = COALESCE(excluded.mime_type, media_assets.mime_type),
            width = COALESCE(excluded.width, media_assets.width),
            height = COALESCE(excluded.height, media_assets.height),
            duration_ms = COALESCE(excluded.duration_ms, media_assets.duration_ms),
            size_bytes = COALESCE(excluded.size_bytes, media_assets.size_bytes),
            updated_at = CURRENT_TIMESTAMP",
        params![
            Uuid::new_v4().to_string(),
            note_id,
            asset.source_asset_id,
            asset.media_type,
            asset.original_url,
            STORAGE_ROOT_ID,
            asset.mime_type,
            asset.width,
            asset.height,
            asset.duration_ms,
            asset.size_bytes
        ],
    )?;
    Ok(())
}

fn mark_media_asset_downloading(conn: &Connection, asset_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE media_assets
         SET download_status = 'downloading',
             download_error = NULL,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
        params![asset_id],
    )?;
    Ok(())
}

fn mark_media_asset_downloaded(
    conn: &Connection,
    asset_id: &str,
    relative_path: &str,
    size_bytes: i64,
    mime_type: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE media_assets
         SET download_status = 'downloaded',
             relative_path = ?1,
             size_bytes = ?2,
             mime_type = COALESCE(?3, mime_type),
             downloaded_at = CURRENT_TIMESTAMP,
             download_error = NULL,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?4",
        params![relative_path, size_bytes, mime_type, asset_id],
    )?;
    Ok(())
}

fn mark_media_asset_failed(conn: &Connection, asset_id: &str, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE media_assets
         SET download_status = 'failed',
             download_error = ?1,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?2",
        params![error.chars().take(500).collect::<String>(), asset_id],
    )?;
    Ok(())
}

#[derive(Debug)]
struct MediaDownloadOutcome {
    downloaded: bool,
    detail: String,
}

fn media_download_concurrency(planned: usize) -> usize {
    usize::max(
        1,
        usize::min(planned, XHS_MEDIA_DOWNLOAD_CONCURRENCY.max(1)),
    )
}

async fn download_one_media_target(
    app: AppHandle,
    media_dir: PathBuf,
    client: Client,
    cookie_header: String,
    index: usize,
    target: MediaDownloadTarget,
) -> MediaDownloadOutcome {
    if let Err(error) = open_library(&app).and_then(|(_, conn)| {
        mark_media_asset_downloading(&conn, &target.id)
            .map_err(|db_error| format!("更新媒体下载状态失败：{db_error}"))
    }) {
        return fail_media_download(&app, &target, error);
    }

    match download_media_bytes(&client, &cookie_header, &target).await {
        Ok((bytes, response_mime)) => {
            let mime_type = response_mime.or_else(|| target.mime_type.clone());
            let relative_path = media_relative_path(&target, mime_type.as_deref());
            let absolute_path = absolute_media_path(&media_dir, &relative_path);
            if let Some(parent) = absolute_path.parent() {
                if let Err(error) = fs::create_dir_all(parent) {
                    return fail_media_download(
                        &app,
                        &target,
                        format!(
                            "创建媒体目录失败 {}：{error}",
                            display_path(parent.to_path_buf())
                        ),
                    );
                }
            }
            if let Err(error) = fs::write(&absolute_path, &bytes) {
                return fail_media_download(
                    &app,
                    &target,
                    format!("写入媒体文件失败 {}：{error}", display_path(absolute_path)),
                );
            }

            if let Err(error) = open_library(&app).and_then(|(_, conn)| {
                mark_media_asset_downloaded(
                    &conn,
                    &target.id,
                    &relative_path,
                    bytes.len() as i64,
                    mime_type.as_deref(),
                )
                .map_err(|db_error| format!("记录媒体下载结果失败：{db_error}"))
            }) {
                return fail_media_download(&app, &target, error);
            }

            MediaDownloadOutcome {
                downloaded: true,
                detail: format!(
                    "最近完成：第 {} 个 {}",
                    index + 1,
                    target.note_source_note_id
                ),
            }
        }
        Err(error) => fail_media_download(&app, &target, error),
    }
}

fn fail_media_download(
    app: &AppHandle,
    target: &MediaDownloadTarget,
    error: String,
) -> MediaDownloadOutcome {
    log::warn!(
        "media_download_failed asset_id={} note_id={} error={}",
        target.id,
        target.note_source_note_id,
        error
    );
    if let Err(db_error) = open_library(app).and_then(|(_, conn)| {
        mark_media_asset_failed(&conn, &target.id, &error)
            .map_err(|error| format!("记录媒体下载失败状态失败：{error}"))
    }) {
        log::warn!(
            "media_download_failed_status_write_failed asset_id={} error={}",
            target.id,
            db_error
        );
    }
    MediaDownloadOutcome {
        downloaded: false,
        detail: format!("最近失败：{} ({})", target.note_source_note_id, error),
    }
}

async fn download_media_bytes(
    client: &Client,
    cookie_header: &str,
    target: &MediaDownloadTarget,
) -> Result<(Vec<u8>, Option<String>), String> {
    let mut request = client
        .get(&target.original_url)
        .header(USER_AGENT, XHS_WEB_USER_AGENT)
        .header(ACCEPT, "*/*")
        .header(REFERER, "https://www.xiaohongshu.com/");
    if !cookie_header.trim().is_empty() {
        request = request.header(COOKIE, cookie_header);
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("请求媒体失败：{error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("媒体返回 HTTP {}", status.as_u16()));
    }
    let mime_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取媒体失败：{error}"))?;
    Ok((bytes.to_vec(), mime_type))
}

fn media_relative_path(target: &MediaDownloadTarget, mime_type: Option<&str>) -> String {
    let directory = match target.media_type.as_str() {
        "video" => "videos",
        "file" => "files",
        _ => "images",
    };
    let note_dir = sanitize_path_segment(&target.note_source_note_id);
    let asset_key = target
        .source_asset_id
        .as_deref()
        .unwrap_or(target.id.as_str())
        .trim_start_matches(&target.note_source_note_id)
        .trim_start_matches(':');
    let file_stem = sanitize_path_segment(if asset_key.is_empty() {
        target.id.as_str()
    } else {
        asset_key
    });
    let extension = media_file_extension(&target.media_type, mime_type, &target.original_url);
    format!("{directory}/{note_dir}/{file_stem}.{extension}")
}

fn absolute_media_path(media_dir: &Path, relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .fold(media_dir.to_path_buf(), |path, segment| path.join(segment))
}

fn media_file_extension(media_type: &str, mime_type: Option<&str>, url: &str) -> &'static str {
    let mime = mime_type.unwrap_or_default().to_lowercase();
    let lower_url = url.to_lowercase();
    if media_type == "file" {
        if mime.contains("pdf") || lower_url.contains(".pdf") {
            return "pdf";
        }
        if mime.contains("word") || lower_url.contains(".docx") || lower_url.contains(".doc") {
            return if lower_url.contains(".docx") {
                "docx"
            } else {
                "doc"
            };
        }
        if mime.contains("spreadsheet")
            || mime.contains("excel")
            || lower_url.contains(".xlsx")
            || lower_url.contains(".xls")
        {
            return if lower_url.contains(".xlsx") {
                "xlsx"
            } else {
                "xls"
            };
        }
        if mime.contains("presentation")
            || mime.contains("powerpoint")
            || lower_url.contains(".pptx")
            || lower_url.contains(".ppt")
        {
            return if lower_url.contains(".pptx") {
                "pptx"
            } else {
                "ppt"
            };
        }
        if mime.contains("zip") || lower_url.contains(".zip") {
            return "zip";
        }
        if lower_url.contains(".rar") {
            return "rar";
        }
        if lower_url.contains(".7z") {
            return "7z";
        }
        if mime.contains("csv") || lower_url.contains(".csv") {
            return "csv";
        }
        if mime.contains("text") || lower_url.contains(".txt") {
            return "txt";
        }
        return "bin";
    }
    if media_type == "video" || mime.contains("mp4") || lower_url.contains(".mp4") {
        return "mp4";
    }
    if mime.contains("webp") || lower_url.contains("webp") {
        return "webp";
    }
    if mime.contains("png") || lower_url.contains(".png") {
        return "png";
    }
    if mime.contains("gif") || lower_url.contains(".gif") {
        return "gif";
    }
    "jpg"
}

fn sanitize_path_segment(input: &str) -> String {
    let sanitized = input
        .chars()
        .take(96)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .trim_matches('_')
        .to_string();
    if sanitized.is_empty() {
        "asset".to_string()
    } else {
        sanitized
    }
}

fn infer_mime_type(media_type: &str, url: &str, fallback: Option<&str>) -> Option<String> {
    if let Some(fallback) = fallback {
        if !fallback.trim().is_empty() {
            return Some(fallback.to_string());
        }
    }
    let lower_url = url.to_lowercase();
    if media_type == "video" || lower_url.contains(".mp4") {
        return Some("video/mp4".to_string());
    }
    if lower_url.contains("webp") {
        return Some("image/webp".to_string());
    }
    if lower_url.contains(".png") {
        return Some("image/png".to_string());
    }
    if lower_url.contains(".gif") {
        return Some("image/gif".to_string());
    }
    if media_type == "image" || media_type == "cover" {
        return Some("image/jpeg".to_string());
    }
    if media_type == "file" {
        if lower_url.contains(".pdf") {
            return Some("application/pdf".to_string());
        }
        if lower_url.contains(".docx") {
            return Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    .to_string(),
            );
        }
        if lower_url.contains(".xlsx") {
            return Some(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
            );
        }
        if lower_url.contains(".pptx") {
            return Some(
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                    .to_string(),
            );
        }
        if lower_url.contains(".zip") {
            return Some("application/zip".to_string());
        }
    }
    None
}

fn first_json_path_i64(value: &Value, paths: &[&[&str]]) -> Option<i64> {
    paths.iter().find_map(|path| json_path_i64(value, path))
}

fn json_path_i64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cursor = value;
    for segment in path {
        if let Ok(index) = segment.parse::<usize>() {
            cursor = cursor.as_array()?.get(index)?;
        } else {
            cursor = cursor.get(*segment)?;
        }
    }
    json_scalar_to_i64(cursor)
}

fn json_scalar_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().map(|value| value.round() as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok().or_else(|| {
            text.trim()
                .parse::<f64>()
                .ok()
                .map(|value| value.round() as i64)
        }),
        _ => None,
    }
}

fn normalize_duration_ms(raw: i64) -> i64 {
    if raw > 0 && raw < 10_000 {
        raw * 1000
    } else {
        raw
    }
}

fn navigate_xhs_window<R: Runtime>(window: &WebviewWindow<R>, url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|error| error.to_string())?;
    window
        .navigate(parsed)
        .map_err(|error| format!("打开小红书页面失败：{error}"))?;
    std::thread::sleep(Duration::from_millis(1200));
    Ok(())
}

fn wait_for_xhs_window<R: Runtime>(
    window: &WebviewWindow<R>,
    description: &str,
    condition_script: &str,
    timeout: Duration,
) -> Result<(), String> {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        match eval_xhs_window_value(window, condition_script) {
            Ok(value) if value.as_bool().unwrap_or(false) => return Ok(()),
            Ok(_) | Err(_) => {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(format!(
        "等待小红书{description}加载超时。请确认登录窗口页面正常显示。"
    ))
}

fn eval_xhs_window_value<R: Runtime>(
    window: &WebviewWindow<R>,
    script: &str,
) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    window
        .eval_with_callback(script, move |payload| {
            let _ = tx.send(payload);
        })
        .map_err(|error| format!("读取小红书页面数据失败：{error}"))?;

    let payload = rx
        .recv_timeout(XHS_EVAL_TIMEOUT)
        .map_err(|_| "读取小红书页面数据超时。".to_string())?;
    parse_eval_payload(&payload)
}

fn parse_eval_payload(payload: &str) -> Result<Value, String> {
    let value = serde_json::from_str::<Value>(payload)
        .unwrap_or_else(|_| Value::String(payload.to_string()));
    if let Some(inner) = value.as_str() {
        return serde_json::from_str::<Value>(inner)
            .or_else(|_| Ok(Value::String(inner.to_string())));
    }
    Ok(value)
}

fn merge_account_info_into_session_result(
    result: &mut XhsSessionTestResult,
    account: XhsAccountInfo,
) {
    let existing_id = result
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let incoming_id = account
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let same_account = match (existing_id.as_deref(), incoming_id.as_deref()) {
        (Some(existing), Some(incoming)) => existing == incoming,
        (None, Some(_)) => true,
        _ => false,
    };

    if !same_account {
        log::warn!(
            "xhs_account_merge_skipped existing_id={:?} incoming_id={:?}",
            existing_id,
            incoming_id
        );
        return;
    }

    if result
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        result.account_id = incoming_id;
    }
    if account.nickname.is_some() {
        result.account_name = account.nickname;
    }
    if account.avatar_url.is_some() {
        result.avatar_url = account.avatar_url;
    }
    if let Some(name) = result.account_name.as_deref() {
        result.account_hint = Some(format!("已连接 {name}"));
    } else if let Some(id) = result.account_id.as_deref() {
        result.account_hint = Some(format!("账号 ID {id}"));
    }
}

fn merge_stored_account_into_session_result(
    result: &mut XhsSessionTestResult,
    stored: &StoredXhsSession,
) {
    if result
        .account_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        result.account_id = Some(stored.source_account_id.clone());
    }
    if result
        .account_name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        result.account_name = stored.display_name.clone();
    }
    if result
        .avatar_url
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        result.avatar_url = stored.avatar_url.clone();
    }
    if let Some(name) = result.account_name.as_deref() {
        result.account_hint = Some(format!("已连接 {name}"));
    }
}

fn account_info_from_cookie_and_html(cookie: &str, html: &str) -> XhsAccountInfo {
    let user_id = cookie_value(cookie, "x-user-id-creator.xiaohongshu.com")
        .or_else(|| json_string_field(html, "userId"))
        .or_else(|| json_string_field(html, "user_id"));
    XhsAccountInfo {
        user_id,
        nickname: None,
        avatar_url: None,
    }
}

fn cookie_value(cookie: &str, key: &str) -> Option<String> {
    cookie.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if name.trim() == key && !value.trim().is_empty() {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn json_string_field(input: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let key_index = input.find(&needle)?;
    let after_key = &input[key_index + needle.len()..];
    let colon_index = after_key.find(':')?;
    let after_colon = after_key[colon_index + 1..].trim_start();
    let value_start = after_colon.strip_prefix('"')?;
    let mut escaped = false;
    for (index, ch) in value_start.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            let value = value_start[..index]
                .replace("\\u002F", "/")
                .replace("\\/", "/")
                .replace("\\\"", "\"");
            return if value.trim().is_empty() {
                None
            } else {
                Some(value)
            };
        }
    }
    None
}

fn xhs_session_key_id(user_id: &str) -> String {
    format!("xhs:{user_id}:web-session")
}

fn ensure_keyring_store() -> Result<(), String> {
    static KEYRING_INIT: OnceLock<Result<(), String>> = OnceLock::new();
    KEYRING_INIT
        .get_or_init(|| {
            keyring::use_native_store(true)
                .map_err(|error| format!("初始化系统 Keychain 失败：{error}"))
        })
        .clone()
}

fn write_session_secret(key_id: &str, secret: &str) -> Result<(), String> {
    ensure_keyring_store()?;
    KeyringEntry::new(KEYRING_SERVICE, key_id)
        .map_err(|error| format!("创建 Keychain 条目失败：{error}"))?
        .set_password(secret)
        .map_err(|error| format!("写入 Keychain 失败：{error}"))
}

fn read_session_secret(key_id: &str) -> Result<String, String> {
    ensure_keyring_store()?;
    KeyringEntry::new(KEYRING_SERVICE, key_id)
        .map_err(|error| format!("创建 Keychain 条目失败：{error}"))?
        .get_password()
        .map_err(|error| format!("读取 Keychain 失败：{error}"))
}

fn save_xhs_session(
    app: &AppHandle,
    cookie_header: &str,
    result: &XhsSessionTestResult,
) -> Result<(), String> {
    ensure_active_profile_for_xhs_session(app, result)?;
    let (_, conn) = open_library(app)?;
    save_xhs_session_with_conn(&conn, cookie_header, result).map_err(|error| error.to_string())
}

fn save_xhs_session_with_conn(
    conn: &Connection,
    cookie_header: &str,
    result: &XhsSessionTestResult,
) -> rusqlite::Result<()> {
    if !result.ok {
        return Ok(());
    }
    let Some(user_id) = result
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let account_id = format!("xhs:{user_id}");
    let session_key_id = xhs_session_key_id(user_id);
    let keychain_saved = match write_session_secret(&session_key_id, cookie_header.trim()) {
        Ok(()) => true,
        Err(error) => {
            log::warn!("xhs_session_keyring_write_failed key_id={session_key_id} error={error}");
            false
        }
    };
    let session_storage = if keychain_saved {
        "keychain"
    } else {
        "sqlite_fallback"
    };
    let sqlite_cookie = if keychain_saved {
        ""
    } else {
        cookie_header.trim()
    };
    conn.execute(
        "INSERT INTO accounts (
            id, source, source_account_id, display_name, avatar_url, session_status,
            session_cookie, session_key_id, session_storage, session_checked_at, last_login_at
         )
         VALUES (?1, 'xhs', ?2, ?3, ?4, 'connected', ?5, ?6, ?7, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(source, source_account_id) DO UPDATE SET
            display_name = COALESCE(excluded.display_name, accounts.display_name),
            avatar_url = COALESCE(excluded.avatar_url, accounts.avatar_url),
            session_status = 'connected',
            session_cookie = excluded.session_cookie,
            session_key_id = excluded.session_key_id,
            session_storage = excluded.session_storage,
            session_checked_at = CURRENT_TIMESTAMP,
            last_login_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP",
        params![
            account_id,
            user_id,
            result.account_name.as_deref(),
            result.avatar_url.as_deref(),
            sqlite_cookie,
            session_key_id,
            session_storage
        ],
    )?;
    Ok(())
}

fn read_latest_xhs_stored_session(conn: &Connection) -> rusqlite::Result<Option<StoredXhsSession>> {
    let stored = conn
        .query_row(
            "SELECT source_account_id, display_name, avatar_url,
                session_cookie, session_key_id, COALESCE(session_storage, 'sqlite')
         FROM accounts
         WHERE source = 'xhs'
           AND (
                COALESCE(session_key_id, '') <> ''
                OR COALESCE(session_cookie, '') <> ''
           )
         ORDER BY
           datetime(COALESCE(session_checked_at, last_login_at, updated_at, created_at)) DESC,
           datetime(updated_at) DESC
         LIMIT 1",
            [],
            |row| {
                Ok(StoredXhsSession {
                    source_account_id: row.get(0)?,
                    display_name: row.get(1)?,
                    avatar_url: row.get(2)?,
                    session_cookie: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    session_key_id: row.get(4)?,
                    session_storage: row.get(5)?,
                })
            },
        )
        .optional()?;

    Ok(stored.and_then(|mut session| {
        if let Some(key_id) = session
            .session_key_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            match read_session_secret(key_id) {
                Ok(secret) if !secret.trim().is_empty() => {
                    session.session_cookie = secret;
                    session.session_storage = "keychain".to_string();
                }
                Ok(_) => log::warn!("xhs_session_keyring_empty key_id={key_id}"),
                Err(error) => {
                    log::warn!("xhs_session_keyring_read_failed key_id={key_id} error={error}")
                }
            }
        }

        if session.session_cookie.trim().is_empty() {
            None
        } else {
            Some(session)
        }
    }))
}

fn mark_xhs_session_status(
    conn: &Connection,
    source_account_id: &str,
    status: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE accounts
         SET session_status = ?2,
             session_checked_at = CURRENT_TIMESTAMP,
             updated_at = CURRENT_TIMESTAMP
         WHERE source = 'xhs' AND source_account_id = ?1",
        params![source_account_id, status],
    )?;
    Ok(())
}

fn read_saved_xhs_cookie(app: &AppHandle) -> Result<Option<String>, String> {
    let (_, conn) = open_library(app)?;
    read_latest_xhs_stored_session(&conn)
        .map(|stored| stored.map(|session| session.session_cookie))
        .map_err(|error| error.to_string())
}

fn restore_saved_xhs_cookie_to_window<R: Runtime>(
    app: &AppHandle,
    window: &WebviewWindow<R>,
) -> Result<String, String> {
    let cookie_header = read_saved_xhs_cookie(app)?.ok_or_else(|| {
        "没有从内置登录窗口或本地保存记录读到小红书登录态。请重新打开登录窗口并完成登录。"
            .to_string()
    })?;
    let injected = inject_xhs_cookie_header(window, &cookie_header)?;
    log::info!(
        "xhs_saved_cookie_injected key_count={} injected={}",
        cookie_keys(&cookie_header).len(),
        injected
    );
    Ok(cookie_header)
}

fn inject_xhs_cookie_header<R: Runtime>(
    window: &WebviewWindow<R>,
    cookie_header: &str,
) -> Result<usize, String> {
    let mut injected = 0usize;
    for (name, value) in xhs_cookie_pairs(cookie_header) {
        let cookie = Cookie::build((name, value))
            .domain("xiaohongshu.com")
            .path("/")
            .secure(true)
            .build();
        window
            .set_cookie(cookie)
            .map_err(|error| format!("写入内置登录窗口 Cookie 失败：{error}"))?;
        injected += 1;
    }
    if injected == 0 {
        return Err("本地保存的登录态里没有可写入的 Cookie。请重新登录。".to_string());
    }
    Ok(injected)
}

fn xhs_cookie_pairs(cookie_header: &str) -> Vec<(String, String)> {
    cookie_header
        .split(';')
        .filter_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty()
                || value.is_empty()
                || name
                    .chars()
                    .any(|ch| ch.is_ascii_control() || matches!(ch, ';' | ',' | ' '))
            {
                return None;
            }
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

fn read_current_or_saved_xhs_cookie(app: &AppHandle) -> Result<Option<String>, String> {
    if let Some(window) = app.get_webview_window(XHS_LOGIN_WINDOW_LABEL) {
        match read_xhs_cookie_header(window) {
            Ok((cookie, _)) if !cookie.trim().is_empty() => return Ok(Some(cookie)),
            Ok(_) => {}
            Err(error) => log::warn!("xhs_cookie_window_read_failed {error}"),
        }
    }

    read_saved_xhs_cookie(app)
}

fn script_with_unwrap(script: &str) -> String {
    script.replace("__UNWRAP_JS__", XHS_UNWRAP_JS)
}

fn cookie_keys(cookie: &str) -> Vec<String> {
    cookie
        .split(';')
        .filter_map(|part| {
            part.trim()
                .split_once('=')
                .map(|(key, _)| key.trim().to_string())
        })
        .filter(|key| !key.is_empty())
        .take(40)
        .collect()
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")?;
    let title_start = start + "<title>".len();
    let end = lower[title_start..].find("</title>")? + title_start;
    Some(html[title_start..end].trim().chars().take(120).collect())
}

fn account_hint(cookie_keys: &[String], html: &str, account_id: Option<&str>) -> Option<String> {
    if let Some(account_id) = account_id.filter(|value| !value.trim().is_empty()) {
        return Some(format!("账号 ID {account_id}"));
    }
    if let Some(key) = cookie_keys.iter().find(|key| key.as_str() == "web_session") {
        return Some(format!("检测到 {key}"));
    }
    if let Some(key) = cookie_keys
        .iter()
        .find(|key| key.as_str() == "x-user-id-creator.xiaohongshu.com")
    {
        return Some(format!("检测到 {key}"));
    }
    if html.contains("userId") || html.contains("user_id") {
        return Some("页面中出现用户字段".to_string());
    }
    None
}

fn ensure_active_profile_for_xhs_session(
    app: &AppHandle,
    result: &XhsSessionTestResult,
) -> Result<(), String> {
    let Some(user_id) = result
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let (_, conn) = open_profile_registry(app)?;
    let active = read_active_local_profile(&conn)
        .map_err(|error| format!("读取当前本地账号失败：{error}"))?;
    let active_matches = active.source.as_deref() == Some("xhs")
        && active.source_account_id.as_deref() == Some(user_id);
    if active_matches {
        update_local_profile_from_xhs_session(&conn, &active.id, user_id, result)
            .map_err(|error| format!("更新本地账号失败：{error}"))?;
        return Ok(());
    }

    if let Some(existing) = find_local_profile_by_xhs_id(&conn, user_id)
        .map_err(|error| format!("查找本地账号失败：{error}"))?
    {
        update_local_profile_from_xhs_session(&conn, &existing.id, user_id, result)
            .map_err(|error| format!("更新本地账号失败：{error}"))?;
        set_active_local_profile(&conn, &existing.id)
            .map_err(|error| format!("切换到已绑定账号失败：{error}"))?;
        log::info!("local_profile_switched_for_xhs user_id={user_id}");
        return Ok(());
    }

    let active_unbound = active
        .source_account_id
        .as_deref()
        .unwrap_or_default()
        .is_empty();
    if active_unbound {
        update_local_profile_from_xhs_session(&conn, &active.id, user_id, result)
            .map_err(|error| format!("绑定当前本地账号失败：{error}"))?;
        log::info!("local_profile_bound_to_xhs user_id={user_id}");
        return Ok(());
    }

    let profile_id = format!("xhs:{user_id}");
    let profile_dir = sanitize_path_segment(&format!("xhs_{user_id}"));
    let db_relative_path = format!("profiles/{profile_dir}/library.sqlite");
    let media_relative_path = format!("profiles/{profile_dir}/media");
    let display_name = result
        .account_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("小红书账号");
    conn.execute(
        "INSERT INTO local_profiles (
            id, display_name, source, source_account_id, avatar_url,
            db_relative_path, media_relative_path, is_active, session_status, last_opened_at
         )
         VALUES (?1, ?2, 'xhs', ?3, ?4, ?5, ?6, 0, 'connected', CURRENT_TIMESTAMP)",
        params![
            profile_id,
            display_name,
            user_id,
            result.avatar_url.as_deref(),
            db_relative_path,
            media_relative_path
        ],
    )
    .map_err(|error| format!("创建本地账号失败：{error}"))?;
    set_active_local_profile(&conn, &profile_id)
        .map_err(|error| format!("切换到新本地账号失败：{error}"))?;
    log::info!("local_profile_created_for_xhs user_id={user_id}");
    Ok(())
}

fn active_local_xhs_account_id(app: &AppHandle) -> Result<Option<String>, String> {
    let (_, conn) = open_profile_registry(app)?;
    let active = read_active_local_profile(&conn)
        .map_err(|error| format!("读取当前本地账号失败：{error}"))?;
    Ok((active.source.as_deref() == Some("xhs"))
        .then_some(active.source_account_id)
        .flatten())
}

fn update_local_profile_from_xhs_session(
    conn: &Connection,
    profile_id: &str,
    user_id: &str,
    result: &XhsSessionTestResult,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE local_profiles
         SET source = 'xhs',
             source_account_id = ?2,
             display_name = COALESCE(?3, display_name),
             avatar_url = COALESCE(?4, avatar_url),
             session_status = 'connected',
             last_opened_at = CURRENT_TIMESTAMP,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
        params![
            profile_id,
            user_id,
            result.account_name.as_deref(),
            result.avatar_url.as_deref()
        ],
    )?;
    Ok(())
}

fn mark_active_profile_synced(app: &AppHandle, user_id: &str) {
    let Ok((_, conn)) = open_profile_registry(app) else {
        return;
    };
    if let Err(error) = conn.execute(
        "UPDATE local_profiles
         SET last_sync_at = CURRENT_TIMESTAMP,
             session_status = 'connected',
             updated_at = CURRENT_TIMESTAMP
         WHERE is_active = 1
           AND source = 'xhs'
           AND source_account_id = ?1",
        params![user_id],
    ) {
        log::warn!("local_profile_last_sync_update_failed {error}");
    }
}

fn mark_active_profile_session_status(app: &AppHandle, user_id: &str, status: &str) {
    let Ok((_, conn)) = open_profile_registry(app) else {
        return;
    };
    if let Err(error) = conn.execute(
        "UPDATE local_profiles
         SET session_status = ?2,
             updated_at = CURRENT_TIMESTAMP
         WHERE is_active = 1
           AND source = 'xhs'
           AND source_account_id = ?1",
        params![user_id, status],
    ) {
        log::warn!("local_profile_session_status_update_failed {error}");
    }
}

fn read_xhs_existing_note_ids(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT source_note_id FROM notes WHERE source = 'xhs'")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn read_xhs_sync_checkpoint(
    conn: &Connection,
    user_id: &str,
    mode: &str,
) -> rusqlite::Result<Option<XhsSyncCheckpoint>> {
    conn.query_row(
        "SELECT anchor_source_note_id, reached_end, scanned_count, remote_display_count
         FROM sync_checkpoints
         WHERE source = 'xhs' AND source_account_id = ?1 AND mode = ?2",
        params![user_id, mode],
        |row| {
            Ok(XhsSyncCheckpoint {
                anchor_source_note_id: row.get(0)?,
                reached_end: row.get::<_, i64>(1)? != 0,
                scanned_count: row.get::<_, i64>(2)?.max(0) as usize,
                remote_display_count: row
                    .get::<_, Option<i64>>(3)?
                    .and_then(|count| usize::try_from(count.max(0)).ok()),
            })
        },
    )
    .optional()
}

fn upsert_xhs_sync_checkpoint(
    conn: &Connection,
    user_id: &str,
    mode: &str,
    collect_result: &XhsFavoriteCollectResult,
) -> rusqlite::Result<()> {
    if !collect_result.limit_reached
        && !collect_result.reached_end
        && collect_result.remote_display_count.is_none()
    {
        return Ok(());
    }

    let checkpoint_id = format!("xhs:{user_id}:{mode}");
    let anchor = if collect_result.limit_reached {
        collect_result.last_source_note_id.as_deref()
    } else {
        None
    };
    conn.execute(
        "INSERT INTO sync_checkpoints (
            id, source, source_account_id, mode, anchor_source_note_id, reached_end,
            scanned_count, remote_display_count, last_success_at, stop_reason
         )
         VALUES (?1, 'xhs', ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP, ?8)
         ON CONFLICT(source, source_account_id, mode) DO UPDATE SET
            anchor_source_note_id = excluded.anchor_source_note_id,
            reached_end = excluded.reached_end,
            scanned_count = excluded.scanned_count,
            remote_display_count = COALESCE(excluded.remote_display_count, sync_checkpoints.remote_display_count),
            last_success_at = CURRENT_TIMESTAMP,
            stop_reason = excluded.stop_reason,
            updated_at = CURRENT_TIMESTAMP",
        params![
            checkpoint_id,
            user_id,
            mode,
            anchor,
            if collect_result.reached_end { 1 } else { 0 },
            collect_result.scanned as i64,
            collect_result.remote_display_count.map(|count| count as i64),
            collect_result.stopped_reason
        ],
    )?;
    Ok(())
}

fn mark_unseen_xhs_notes_missing(
    conn: &Connection,
    seen_note_ids: &HashSet<String>,
) -> rusqlite::Result<usize> {
    if seen_note_ids.is_empty() {
        return Ok(0);
    }

    let existing_ids = read_xhs_existing_note_ids(conn)?;
    let mut changed = 0usize;
    for source_note_id in existing_ids {
        if seen_note_ids.contains(&source_note_id) {
            continue;
        }
        changed += conn.execute(
            "UPDATE notes
             SET remote_missing_at = COALESCE(remote_missing_at, CURRENT_TIMESTAMP),
                 remote_status = 'missing_from_favorites',
                 unavailable_reason = COALESCE(unavailable_reason, 'not_seen_in_complete_favorites_sync'),
                 updated_at = CURRENT_TIMESTAMP
             WHERE source = 'xhs'
               AND source_note_id = ?1
               AND remote_status = 'available'",
            params![source_note_id],
        )?;
    }
    Ok(changed)
}

fn mark_seen_xhs_notes_available(
    conn: &Connection,
    seen_note_ids: &HashSet<String>,
    favorite_positions: &[(String, usize)],
) -> rusqlite::Result<()> {
    let position_map: HashMap<&str, i64> = favorite_positions
        .iter()
        .map(|(source_note_id, position)| (source_note_id.as_str(), *position as i64))
        .collect();
    for source_note_id in seen_note_ids {
        let favorite_order = position_map.get(source_note_id.as_str()).copied();
        conn.execute(
            "UPDATE notes
             SET last_seen_at = CURRENT_TIMESTAMP,
                 remote_missing_at = NULL,
                 remote_status = 'available',
                 unavailable_reason = NULL,
                 favorite_order = COALESCE(?2, favorite_order),
                 updated_at = CURRENT_TIMESTAMP
             WHERE source = 'xhs' AND source_note_id = ?1",
            params![source_note_id, favorite_order],
        )?;
    }
    Ok(())
}

fn record_xhs_sync_run(
    conn: &Connection,
    user_id: &str,
    summary: &ImportSummary,
    collect_result: &XhsFavoriteCollectResult,
    remote_missing: usize,
) -> rusqlite::Result<String> {
    let run_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sync_runs (
            id, source, status, finished_at, fetched_count, scanned_count,
            inserted_count, updated_count, skipped_count, existing_skipped_count,
            remote_missing_count, reached_end, limit_reached, stop_reason,
            first_source_note_id, last_source_note_id
         )
         VALUES (
            ?1, 'xhs', 'completed', CURRENT_TIMESTAMP, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13
         )",
        params![
            run_id,
            collect_result.notes.len() as i64,
            collect_result.scanned as i64,
            summary.inserted as i64,
            summary.updated as i64,
            summary.skipped as i64,
            collect_result.existing_skipped as i64,
            remote_missing as i64,
            if collect_result.reached_end { 1 } else { 0 },
            if collect_result.limit_reached { 1 } else { 0 },
            collect_result.stopped_reason,
            collect_result.first_source_note_id,
            collect_result.last_source_note_id
        ],
    )?;

    let account_id = format!("xhs:{user_id}");
    conn.execute(
        "INSERT INTO accounts (id, source, source_account_id, session_status, last_sync_at)
         VALUES (?1, 'xhs', ?2, 'connected', CURRENT_TIMESTAMP)
         ON CONFLICT(source, source_account_id) DO UPDATE SET
            session_status = 'connected',
            last_sync_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP",
        params![account_id, user_id],
    )?;

    Ok(run_id)
}

fn upsert_xhs_album(
    conn: &Connection,
    user_id: &str,
    album: &XhsAlbumDraft,
) -> rusqlite::Result<String> {
    let album_id = format!(
        "xhs:{user_id}:album:{}",
        sanitize_path_segment(&album.source_album_id)
    );
    let name = unique_album_name(conn, &album_id, &album.name)?;
    conn.execute(
        "INSERT INTO albums (
            id, name, description, source, source_album_id, source_account_id,
            source_url, cover_url, note_count, raw_json, last_synced_at
         )
         VALUES (?1, ?2, ?3, 'xhs', ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            source = excluded.source,
            source_album_id = excluded.source_album_id,
            source_account_id = excluded.source_account_id,
            source_url = COALESCE(excluded.source_url, albums.source_url),
            cover_url = COALESCE(excluded.cover_url, albums.cover_url),
            note_count = COALESCE(excluded.note_count, albums.note_count),
            raw_json = excluded.raw_json,
            last_synced_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP",
        params![
            album_id,
            name,
            album.description,
            album.source_album_id,
            user_id,
            album.source_url.as_deref(),
            album.cover_url.as_deref(),
            album.note_count.map(|count| count as i64),
            album.raw_json
        ],
    )?;
    Ok(album_id)
}

fn cleanup_invalid_xhs_albums(conn: &Connection, user_id: &str) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, source_album_id, source_url
         FROM albums
         WHERE source = 'xhs' AND source_account_id = ?1",
    )?;
    let rows = stmt.query_map(params![user_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut album_ids = Vec::new();
    for row in rows {
        let (album_id, source_album_id, source_url) = row?;
        let source_album_id = source_album_id.unwrap_or_default();
        let source_url = source_url.unwrap_or_default();
        let invalid_source = source_album_id.trim().is_empty()
            || source_album_id.to_lowercase().contains("beian.miit.gov.cn");
        if invalid_source || !is_probable_xhs_album_url(&source_url) {
            album_ids.push(album_id);
        }
    }

    let mut deleted = 0usize;
    for album_id in album_ids {
        conn.execute(
            "DELETE FROM album_notes WHERE album_id = ?1",
            params![album_id],
        )?;
        deleted += conn.execute("DELETE FROM albums WHERE id = ?1", params![album_id])?;
    }
    Ok(deleted)
}

fn unique_album_name(conn: &Connection, album_id: &str, desired: &str) -> rusqlite::Result<String> {
    let desired = desired.trim();
    let base = if desired.is_empty() {
        "未命名专辑"
    } else {
        desired
    };
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM albums WHERE name = ?1 LIMIT 1",
            params![base],
            |row| row.get(0),
        )
        .optional()?;
    if existing
        .as_deref()
        .is_none_or(|existing_id| existing_id == album_id)
    {
        return Ok(base.to_string());
    }

    let suffix = album_id.rsplit(':').next().unwrap_or(album_id);
    let short_suffix: String = suffix.chars().take(8).collect();
    let candidate = format!("{base} · {short_suffix}");
    Ok(candidate.chars().take(96).collect())
}

fn count_existing_xhs_notes(
    conn: &Connection,
    source_note_ids: &[String],
) -> rusqlite::Result<usize> {
    let mut count = 0usize;
    let mut seen = HashSet::new();
    let mut stmt =
        conn.prepare("SELECT 1 FROM notes WHERE source = 'xhs' AND source_note_id = ?1")?;
    for source_note_id in source_note_ids {
        if !seen.insert(source_note_id.as_str()) {
            continue;
        }
        if stmt
            .query_row(params![source_note_id], |_| Ok(()))
            .optional()?
            .is_some()
        {
            count += 1;
        }
    }
    Ok(count)
}

fn upsert_album_note_links(
    conn: &Connection,
    album_id: &str,
    note_ids: &[String],
) -> rusqlite::Result<usize> {
    let mut linked = 0usize;
    let mut seen = HashSet::new();
    for (index, note_id) in note_ids.iter().enumerate() {
        if !seen.insert(note_id.as_str()) {
            continue;
        }
        let changed = conn.execute(
            "INSERT INTO album_notes (album_id, note_id, sort_order)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(album_id, note_id) DO UPDATE SET
                sort_order = excluded.sort_order",
            params![album_id, note_id, index as i64],
        )?;
        if changed > 0 {
            linked += 1;
        }
    }
    Ok(linked)
}

fn upsert_xhs_favorite_notes(
    conn: &Connection,
    raw_notes: &[Value],
    app: Option<&AppHandle>,
    planned: usize,
    to_sync: usize,
    scanned: usize,
    existing_skipped: usize,
) -> rusqlite::Result<ImportSummary> {
    let mut summary = ImportSummary {
        inserted: 0,
        updated: 0,
        skipped: 0,
        note_ids: Vec::new(),
    };

    for (index, raw) in raw_notes.iter().enumerate() {
        let Some(source_note_id) = xhs_note_id(raw) else {
            summary.skipped += 1;
            if let Some(app) = app {
                emit_xhs_sync_progress(
                    app,
                    XhsSyncProgress {
                        phase: "writing_library".to_string(),
                        label: "写入本地库".to_string(),
                        detail: format!("正在写入第 {} / {to_sync} 条收藏。", index + 1),
                        planned,
                        scanned,
                        fetched: scanned,
                        to_sync: Some(to_sync),
                        written: index + 1,
                        inserted: summary.inserted,
                        updated: summary.updated,
                        skipped: summary.skipped,
                        existing_skipped,
                        progress: (72 + (((index + 1) * 24) / usize::max(to_sync, 1)).min(24))
                            as u8,
                        indeterminate: false,
                    },
                );
            }
            continue;
        };

        let title = first_json_path_str(
            raw,
            &[
                &["displayTitle"],
                &["display_title"],
                &["title"],
                &["note", "title"],
                &["noteCard", "displayTitle"],
                &["noteCard", "display_title"],
                &["noteCard", "title"],
                &["note_card", "displayTitle"],
                &["note_card", "display_title"],
                &["note_card", "title"],
            ],
        )
        .unwrap_or_else(|| "未命名收藏".to_string());
        let excerpt = first_json_path_str(
            raw,
            &[
                &["desc"],
                &["description"],
                &["content"],
                &["note", "desc"],
                &["note", "description"],
                &["noteCard", "desc"],
                &["noteCard", "description"],
                &["note_card", "desc"],
                &["note_card", "description"],
            ],
        )
        .unwrap_or_else(|| title.clone());
        let author_name = first_json_path_str(
            raw,
            &[
                &["user", "nickname"],
                &["user", "nickName"],
                &["user", "nick_name"],
                &["noteUser", "nickname"],
                &["note_user", "nickname"],
                &["author", "nickname"],
                &["noteCard", "user", "nickname"],
                &["note_card", "user", "nickname"],
            ],
        )
        .unwrap_or_default();
        let cover_url = xhs_cover_url(raw);
        let note_type = xhs_note_type(raw);
        let published_at = xhs_published_at(raw);
        let collected_at = xhs_collected_at(raw);
        let source_url = xhs_note_url(raw, &source_note_id);
        let raw_json = raw.to_string();

        let existing_id: Option<String> = conn
            .query_row(
                "SELECT id FROM notes WHERE source = 'xhs' AND source_note_id = ?1",
                params![source_note_id],
                |row| row.get(0),
            )
            .optional()?;

        let note_id = if let Some(note_id) = existing_id {
            conn.execute(
                "UPDATE notes
                 SET source_url = ?1,
                     title = ?2,
             excerpt = CASE WHEN COALESCE(content, '') <> '' THEN excerpt ELSE ?3 END,
                     author_name = ?4,
                     cover_url = ?5,
                     note_type = ?6,
                     published_at = COALESCE(?7, published_at),
                     collected_at = COALESCE(?8, collected_at),
             raw_json = CASE
                        WHEN COALESCE(content, '') <> ''
                          OR EXISTS (SELECT 1 FROM note_tags nt WHERE nt.note_id = notes.id)
                        THEN raw_json
                        ELSE ?9
                     END,
                     last_seen_at = CURRENT_TIMESTAMP,
                     last_synced_at = CURRENT_TIMESTAMP,
                     updated_at = CURRENT_TIMESTAMP,
                     remote_missing_at = NULL,
                     remote_status = 'available',
                     unavailable_reason = NULL
                 WHERE id = ?10",
                params![
                    source_url,
                    title,
                    excerpt,
                    author_name,
                    cover_url,
                    note_type,
                    published_at,
                    collected_at,
                    raw_json,
                    note_id
                ],
            )?;
            summary.updated += 1;
            note_id
        } else {
            let note_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO notes (
                    id, source, source_note_id, source_url, title, excerpt, author_name,
                    cover_url, note_type, published_at, collected_at, last_seen_at,
                    status, raw_json, remote_status
                 )
                 VALUES (?1, 'xhs', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP,
                         'unread', ?11, 'available')",
                params![
                    note_id,
                    source_note_id,
                    source_url,
                    title,
                    excerpt,
                    author_name,
                    cover_url,
                    note_type,
                    published_at,
                    collected_at,
                    raw_json
                ],
            )?;
            summary.inserted += 1;
            note_id
        };

        if let Some(cover_url) = xhs_cover_url(raw) {
            conn.execute(
                "INSERT INTO media_assets (
                    id, note_id, source, source_asset_id, media_type, original_url,
                    storage_root_id, download_status
                 )
                 VALUES (?1, ?2, 'xhs', ?3, 'cover', ?4, ?5, 'not_downloaded')
                 ON CONFLICT(note_id, source_asset_id) DO UPDATE SET
                    original_url = excluded.original_url,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    Uuid::new_v4().to_string(),
                    note_id,
                    format!("{source_note_id}:cover"),
                    cover_url,
                    STORAGE_ROOT_ID
                ],
            )?;
        }

        upsert_xhs_text_tags(conn, &note_id, &[title.as_str(), excerpt.as_str()])?;

        summary.note_ids.push(note_id.clone());

        if let Some(app) = app {
            emit_xhs_sync_progress(
                app,
                XhsSyncProgress {
                    phase: "writing_library".to_string(),
                    label: "写入本地库".to_string(),
                    detail: format!("正在写入第 {} / {to_sync} 条收藏。", index + 1),
                    planned,
                    scanned,
                    fetched: scanned,
                    to_sync: Some(to_sync),
                    written: index + 1,
                    inserted: summary.inserted,
                    updated: summary.updated,
                    skipped: summary.skipped,
                    existing_skipped,
                    progress: (72 + (((index + 1) * 24) / usize::max(to_sync, 1)).min(24)) as u8,
                    indeterminate: false,
                },
            );
        }
    }

    Ok(summary)
}

fn xhs_note_id(value: &Value) -> Option<String> {
    first_json_path_str(
        value,
        &[
            &["noteId"],
            &["note_id"],
            &["id"],
            &["note", "noteId"],
            &["note", "note_id"],
            &["note", "id"],
            &["noteCard", "noteId"],
            &["noteCard", "note_id"],
            &["noteCard", "id"],
            &["note_card", "noteId"],
            &["note_card", "note_id"],
            &["note_card", "id"],
        ],
    )
}

fn xhs_note_url(value: &Value, source_note_id: &str) -> String {
    let base = format!("https://www.xiaohongshu.com/explore/{source_note_id}");
    if let Some(url) = first_json_path_str(
        value,
        &[
            &["sourceUrl"],
            &["source_url"],
            &["noteUrl"],
            &["note_url"],
            &["href"],
        ],
    )
    .and_then(|url| normalize_xhs_note_url(&url, source_note_id))
    {
        return url;
    }

    let Some(token) = first_json_path_str(value, &[&["xsecToken"], &["xsec_token"]]) else {
        return base;
    };
    let source = first_json_path_str(value, &[&["xsecSource"], &["xsec_source"]])
        .unwrap_or_else(|| "pc_collect".to_string());
    format!(
        "{base}?xsec_token={}&xsec_source={}",
        encode_url_query_value(&token),
        encode_url_query_value(&source)
    )
}

fn normalize_xhs_note_url(raw_url: &str, source_note_id: &str) -> Option<String> {
    let value = raw_url.trim();
    if value.is_empty() || !value.contains(source_note_id) {
        return None;
    }
    let absolute = if value.starts_with("https://www.xiaohongshu.com/") {
        value.to_string()
    } else if value.starts_with('/') {
        format!("https://www.xiaohongshu.com{value}")
    } else {
        return None;
    };

    let query = absolute
        .find('?')
        .map(|index| &absolute[index..])
        .unwrap_or("");
    Some(format!(
        "https://www.xiaohongshu.com/explore/{source_note_id}{query}"
    ))
}

fn encode_url_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn xhs_note_type(value: &Value) -> String {
    let raw_type = first_json_path_str(
        value,
        &[
            &["noteType"],
            &["note_type"],
            &["type"],
            &["note", "type"],
            &["note", "noteType"],
        ],
    )
    .unwrap_or_default()
    .to_lowercase();

    if raw_type.contains("video") || raw_type == "1" {
        return "video".to_string();
    }
    if raw_type.contains("image")
        || raw_type.contains("normal")
        || value.get("imageList").and_then(Value::as_array).is_some()
        || value.get("images").and_then(Value::as_array).is_some()
    {
        return "image".to_string();
    }
    "unknown".to_string()
}

fn xhs_published_at(value: &Value) -> Option<String> {
    xhs_time_at(
        value,
        &[
            &["publishTime"],
            &["publish_time"],
            &["publishedAt"],
            &["published_at"],
            &["createTime"],
            &["create_time"],
            &["timestamp"],
            &["time"],
            &["note", "publishTime"],
            &["note", "createTime"],
            &["noteCard", "publishTime"],
            &["noteCard", "time"],
            &["note_card", "publish_time"],
            &["note_card", "time"],
        ],
    )
}

fn xhs_collected_at(value: &Value) -> Option<String> {
    xhs_time_at(
        value,
        &[
            &["collectTime"],
            &["collect_time"],
            &["collectedAt"],
            &["collected_at"],
            &["favoriteTime"],
            &["favorite_time"],
            &["favTime"],
            &["fav_time"],
            &["userInteract", "collectTime"],
            &["user_interact", "collect_time"],
        ],
    )
}

fn xhs_time_at(value: &Value, paths: &[&[&str]]) -> Option<String> {
    first_json_path_str(value, paths).and_then(|value| normalize_xhs_time(&value))
}

fn normalize_xhs_time(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime.with_timezone(&Utc).to_rfc3339());
    }

    if value.chars().all(|ch| ch.is_ascii_digit()) {
        if let Ok(number) = value.parse::<i64>() {
            let millis = if number > 10_000_000_000 {
                number
            } else {
                number * 1000
            };
            if let Some(datetime) = Utc.timestamp_millis_opt(millis).single() {
                return Some(datetime.to_rfc3339());
            }
        }
    }

    Some(value.to_string())
}

fn xhs_cover_url(value: &Value) -> Option<String> {
    first_json_path_str(
        value,
        &[
            &["cover", "url"],
            &["cover", "urlDefault"],
            &["cover", "url_default"],
            &["cover", "infoList", "0", "url"],
            &["image", "url"],
            &["image", "urlDefault"],
            &["noteCard", "cover", "url"],
            &["noteCard", "cover", "urlDefault"],
            &["noteCard", "cover", "infoList", "0", "url"],
            &["note_card", "cover", "url"],
            &["note_card", "cover", "url_default"],
            &["note_card", "cover", "infoList", "0", "url"],
        ],
    )
    .or_else(|| first_array_image_url(value, "imageList"))
    .or_else(|| first_array_image_url(value, "images"))
}

fn first_array_image_url(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_array).and_then(|items| {
        items.iter().find_map(|item| {
            first_json_path_str(
                item,
                &[
                    &["url"],
                    &["urlDefault"],
                    &["url_default"],
                    &["infoList", "0", "url"],
                ],
            )
        })
    })
}

fn first_json_path_str(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| json_path_str(value, path))
}

fn json_path_str(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for segment in path {
        if let Ok(index) = segment.parse::<usize>() {
            cursor = cursor.as_array()?.get(index)?;
        } else {
            cursor = cursor.get(*segment)?;
        }
    }
    json_scalar_to_string(cursor)
}

fn json_scalar_to_string(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return None,
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
