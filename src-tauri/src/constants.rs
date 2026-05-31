use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
use std::time::Duration;
pub(crate) const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");
pub(crate) const STORAGE_ROOT_ID: &str = "default-media";
pub(crate) const DEFAULT_PROFILE_ID: &str = "local:default";
pub(crate) const PROFILE_REGISTRY_DB: &str = "profiles.sqlite";
pub(crate) const KEYRING_SERVICE: &str = "com.guesswhatstudio.xhscollection.xhs";
pub(crate) const AI_KEYRING_SERVICE: &str = "com.guesswhatstudio.xhscollection.ai";
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";
pub(crate) const XHS_LOGIN_WINDOW_LABEL: &str = "xhs-login";
pub(crate) const XHS_DETAIL_WINDOW_LABEL: &str = "xhs-detail-fetch";
pub(crate) const XHS_LOGIN_URL: &str = "https://www.xiaohongshu.com/explore";
pub(crate) const XHS_COOKIE_URLS: [&str; 2] =
    ["https://www.xiaohongshu.com/", "https://xiaohongshu.com/"];
pub(crate) const XHS_COOKIE_STORE_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const XHS_DOCUMENT_COOKIE_TIMEOUT: Duration = Duration::from_secs(4);
pub(crate) const XHS_EVAL_TIMEOUT: Duration = Duration::from_secs(4);
pub(crate) const XHS_PAGE_READY_TIMEOUT: Duration = Duration::from_secs(18);
pub(crate) const XHS_FAVORITES_MAX_SCROLL_ATTEMPTS: usize = 1200;
pub(crate) const XHS_FAVORITES_STABLE_ATTEMPTS: usize = 4;
pub(crate) const XHS_FAVORITES_SCROLL_DELAY: Duration = Duration::from_millis(1800);
pub(crate) const XHS_ALBUMS_MAX_SCROLL_ATTEMPTS: usize = 180;
pub(crate) const XHS_ALBUMS_STABLE_ATTEMPTS: usize = 3;
pub(crate) const XHS_ALBUMS_SCROLL_DELAY: Duration = Duration::from_millis(1200);
pub(crate) const XHS_WEB_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36 Edg/148.0.0.0";
pub(crate) const XHS_DOWNLOAD_DEFAULT_LIMIT: usize = 30;
pub(crate) const XHS_MEDIA_DOWNLOAD_CONCURRENCY: usize = 4;
pub(crate) const XHS_REQUEST_DELAY: Duration = Duration::from_millis(900);
pub(crate) const AI_SETTINGS_ID: &str = "default";
pub(crate) const AI_DEFAULT_PROVIDER: &str = "openai_compatible";
pub(crate) const AI_DEFAULT_BASE_URL: &str = "https://api.deepseek.com/v1";
pub(crate) const AI_DEFAULT_MODEL: &str = "deepseek-chat";
pub(crate) const AI_DEFAULT_TEMPERATURE: f64 = 0.2;
pub(crate) const AI_DEFAULT_MAX_TOKENS: i64 = 4096;
pub(crate) const AI_CLASSIFY_BATCH_SIZE: usize = 8;
pub(crate) const AI_CLASSIFY_RETRY_BATCH_SIZE: usize = 2;
pub(crate) const AI_UNCATEGORIZED_DEFAULT_LIMIT: usize = 120;
pub(crate) const AI_TAG_GROUP_DEFAULT_LIMIT: usize = 260;
pub(crate) const AI_NOTE_TEXT_LIMIT: usize = 900;
pub(crate) const AI_MIN_ASSIGNMENT_CONFIDENCE: f64 = 0.45;
pub(crate) const AI_MIN_NEW_CATEGORY_CONFIDENCE: f64 = 0.65;

pub(crate) static XHS_POST_SYNC_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();
pub(crate) static XHS_SYNC_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
pub(crate) static AI_TASK_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn reset_xhs_sync_cancel() {
    XHS_SYNC_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
}

pub(crate) fn request_xhs_sync_cancel() {
    XHS_SYNC_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

pub(crate) fn xhs_sync_cancel_requested() -> bool {
    XHS_SYNC_CANCEL_REQUESTED.load(Ordering::SeqCst)
}

pub(crate) fn reset_ai_task_cancel() {
    AI_TASK_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
}

pub(crate) fn request_ai_task_cancel() {
    AI_TASK_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

pub(crate) fn ai_task_cancel_requested() -> bool {
    AI_TASK_CANCEL_REQUESTED.load(Ordering::SeqCst)
}
