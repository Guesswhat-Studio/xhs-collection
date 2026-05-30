use chrono::{DateTime, TimeZone, Utc};
use keyring_core::Entry as KeyringEntry;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, COOKIE, REFERER, USER_AGENT};
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    AppHandle, Emitter, Manager, Runtime, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use uuid::Uuid;

const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");
const STORAGE_ROOT_ID: &str = "default-media";
const DEFAULT_PROFILE_ID: &str = "local:default";
const PROFILE_REGISTRY_DB: &str = "profiles.sqlite";
const KEYRING_SERVICE: &str = "com.guesswhatstudio.xhscollection.xhs";
const AI_KEYRING_SERVICE: &str = "com.guesswhatstudio.xhscollection.ai";
const MAIN_WINDOW_LABEL: &str = "main";
const XHS_LOGIN_WINDOW_LABEL: &str = "xhs-login";
const XHS_LOGIN_URL: &str = "https://www.xiaohongshu.com/explore";
const XHS_COOKIE_URLS: [&str; 2] = ["https://www.xiaohongshu.com/", "https://xiaohongshu.com/"];
const XHS_COOKIE_STORE_TIMEOUT: Duration = Duration::from_secs(8);
const XHS_DOCUMENT_COOKIE_TIMEOUT: Duration = Duration::from_secs(4);
const XHS_EVAL_TIMEOUT: Duration = Duration::from_secs(4);
const XHS_PAGE_READY_TIMEOUT: Duration = Duration::from_secs(18);
const XHS_FAVORITES_MAX_SCROLL_ATTEMPTS: usize = 1200;
const XHS_FAVORITES_STABLE_ATTEMPTS: usize = 4;
const XHS_FAVORITES_SCROLL_DELAY: Duration = Duration::from_millis(1800);
const XHS_WEB_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36 Edg/148.0.0.0";
const XHS_DETAIL_DEFAULT_LIMIT: usize = 100;
const XHS_DOWNLOAD_DEFAULT_LIMIT: usize = 30;
const XHS_REQUEST_DELAY: Duration = Duration::from_millis(900);
const AI_SETTINGS_ID: &str = "default";
const AI_DEFAULT_PROVIDER: &str = "openai_compatible";
const AI_DEFAULT_BASE_URL: &str = "https://api.deepseek.com/v1";
const AI_DEFAULT_MODEL: &str = "deepseek-chat";
const AI_DEFAULT_TEMPERATURE: f64 = 0.2;
const AI_DEFAULT_MAX_TOKENS: i64 = 4096;
const AI_CLASSIFY_BATCH_SIZE: usize = 24;
const AI_UNCATEGORIZED_DEFAULT_LIMIT: usize = 120;
const AI_TAG_GROUP_DEFAULT_LIMIT: usize = 260;
const AI_NOTE_TEXT_LIMIT: usize = 900;

static XHS_POST_SYNC_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();

const XHS_UNWRAP_JS: &str = r#"
function unwrap(obj, depth) {
  if (depth > 6 || obj === null || obj === undefined) return obj;
  if (typeof obj !== 'object') return obj;
  if ('_value' in obj && 'dep' in obj) return unwrap(obj._value, depth + 1);
  if ('value' in obj && 'dep' in obj) return unwrap(obj.value, depth + 1);
  if (Array.isArray(obj)) return obj.map(item => unwrap(item, depth + 1));
  const result = {};
  for (const key of Object.keys(obj)) {
    if (key === 'dep' || key.startsWith('__')) continue;
    try { result[key] = unwrap(obj[key], depth + 1); } catch (_) {}
  }
  return result;
}
"#;

const XHS_SELF_INFO_SCRIPT: &str = r#"
(() => {
__UNWRAP_JS__
  const state = window.__INITIAL_STATE__;
  if (!state) return { ready: false };

  const candidates = [
    state.user && state.user.userInfo,
    state.user && state.user.currentUser,
    state.user && state.user.loginUser,
    state.user && state.user.info,
    state.user,
    state.sidebar && state.sidebar.user,
    state.app && state.app.user,
    state.user && state.user.userPageData
  ];

  function pickUser(data) {
    if (!data || typeof data !== 'object') return null;
    const basic = data.basicInfo || data.basic_info || data.userInfo || data.user_info || data;
    const userPageBasic = data.userPageData && (data.userPageData.basicInfo || data.userPageData.basic_info);
    const source = userPageBasic || basic;
    const userId = source.userId || source.user_id || source.id || data.userId || data.user_id || data.id || '';
    const nickname = source.nickname || source.nick_name || source.name || data.nickname || data.nick_name || '';
    const avatar = source.avatar || source.image || source.avatarUrl || source.avatar_url || source.images || data.avatar || data.avatarUrl || data.avatar_url || '';
    if (userId || nickname) return { userId: String(userId || ''), nickname: String(nickname || ''), avatarUrl: String(avatar || '') };
    return null;
  }

  for (const candidate of candidates) {
    const picked = pickUser(unwrap(candidate, 0));
    if (picked) return { ready: true, ...picked };
  }

  return { ready: true, userId: '', nickname: '' };
})()
"#;

const XHS_FAVORITES_SCRIPT: &str = r#"
(() => {
__UNWRAP_JS__
  const debug = { href: window.location.href, title: document.title || '', userKeys: [], candidatePaths: [], linkCount: 0, cardCount: 0 };

  function get(obj, path) {
    let cur = obj;
    for (const key of path) {
      if (cur === null || cur === undefined) return undefined;
      cur = cur[key];
    }
    return cur;
  }

  function first(obj, paths) {
    for (const path of paths) {
      const value = get(obj, path);
      if (value !== null && value !== undefined && String(value).trim()) return String(value).trim();
    }
    return '';
  }

  function noteIdOf(item) {
    return first(item, [
      ['noteId'], ['note_id'], ['id'],
      ['note', 'noteId'], ['note', 'note_id'], ['note', 'id'],
      ['noteCard', 'noteId'], ['noteCard', 'note_id'], ['noteCard', 'id'],
      ['note_card', 'noteId'], ['note_card', 'note_id'], ['note_card', 'id'],
    ]);
  }

  function noteScore(item) {
    if (!item || typeof item !== 'object') return 0;
    const id = noteIdOf(item);
    if (!id || id.length < 8) return 0;
    let score = 1;
    if (first(item, [['displayTitle'], ['display_title'], ['title'], ['noteCard', 'displayTitle'], ['note_card', 'display_title']])) score += 1;
    if (first(item, [['cover', 'url'], ['noteCard', 'cover', 'url'], ['note_card', 'cover', 'url']])) score += 1;
    return score;
  }

  function arrayItems(value) {
    const data = unwrap(value, 0);
    if (Array.isArray(data)) return data;
    if (data && typeof data === 'object') {
      for (const key of ['value', '_value', 'data', 'list', 'items', 'feeds', 'notes']) {
        if (Array.isArray(data[key])) return data[key];
      }
    }
    return [];
  }

  function collectArrays(obj, path, depth, out, seen) {
    if (!obj || depth > 6) return;
    const data = unwrap(obj, 0);
    if (!data || typeof data !== 'object') return;
    if (seen.has(data)) return;
    seen.add(data);
    if (Array.isArray(data)) {
      const scored = data
        .filter(item => item && typeof item === 'object')
        .map(item => ({ item, score: noteScore(item) }))
        .filter(entry => entry.score > 0);
      if (scored.length) {
        out.push({ path, score: scored.reduce((sum, entry) => sum + entry.score, 0), items: scored.map(entry => entry.item) });
      }
      return;
    }
    const keys = Object.keys(data).sort((a, b) => {
      const rank = key => /fav|collect|collection|note|feed|list|item/i.test(key) ? 0 : 1;
      return rank(a) - rank(b);
    });
    for (const key of keys) {
      if (key === 'dep' || key.startsWith('__')) continue;
      collectArrays(data[key], path ? `${path}.${key}` : key, depth + 1, out, seen);
    }
  }

  function notesFromState() {
    const state = window.__INITIAL_STATE__;
    if (!state) return [];
    if (state.user && typeof state.user === 'object') debug.userKeys = Object.keys(unwrap(state.user, 0)).slice(0, 40);
    const u = state.user || {};
    const sources = [
      u.collect, u.collectNotes, u.collection, u.collections, u.favorite, u.favorites,
      u.userCollectNotes, u.favNotes, u.notes, u.feeds, u.profile && u.profile.notes,
      u.userPageData && u.userPageData.notes,
    ];
    for (const src of sources) {
      const items = arrayItems(src);
      if (items.some(item => noteScore(item) > 0)) return items;
    }
    const candidates = [];
    collectArrays(state.user, 'user', 0, candidates, new WeakSet());
    candidates.sort((a, b) => b.score - a.score);
    debug.candidatePaths = candidates.slice(0, 8).map(candidate => `${candidate.path}:${candidate.items.length}`);
    return candidates[0] ? candidates[0].items : [];
  }

  function notesFromDom() {
    const cards = document.querySelectorAll('section.note-item, [class*="note-item"], [class*="note-card"], a[href*="/explore/"], a[href*="/discovery/item/"], a[href*="/user/profile/"]');
    debug.cardCount = cards.length;
    debug.linkCount = document.querySelectorAll('a[href*="/explore/"], a[href*="/discovery/item/"]').length;
    return Array.from(cards).map(card => {
      const links = (card.matches && (card.matches('a[href*="/explore/"]') || card.matches('a[href*="/discovery/item/"]') || card.matches('a[href*="/user/profile/"]')))
        ? [card]
        : Array.from(card.querySelectorAll('a[href*="/explore/"], a[href*="/discovery/item/"], a[href*="/user/profile/"]'));
      const noteIdFromHref = href => {
        const text = String(href || '');
        return (text.match(/\/(?:explore|discovery\/item)\/([a-zA-Z0-9]+)/) || [])[1]
          || (text.match(/\/user\/profile\/[a-zA-Z0-9]+\/([a-zA-Z0-9]+)/) || [])[1]
          || '';
      };
      const noteLinks = links.filter(link => noteIdFromHref(link.getAttribute('href') || link.href || ''));
      const link = noteLinks.find(link => (link.href || link.getAttribute('href') || '').includes('xsec_token='))
        || noteLinks.find(link => (link.href || link.getAttribute('href') || '').includes('/user/profile/'))
        || noteLinks[0]
        || null;
      const href = link ? (link.getAttribute('href') || link.href || '') : '';
      const sourceUrl = href ? new URL(href, window.location.origin).href : '';
      let xsecToken = '';
      let xsecSource = '';
      try {
        const parsed = sourceUrl ? new URL(sourceUrl) : null;
        xsecToken = parsed ? (parsed.searchParams.get('xsec_token') || '') : '';
        xsecSource = parsed ? (parsed.searchParams.get('xsec_source') || '') : '';
      } catch (_) {}
      const titleEl = card.querySelector('[class*="title"]');
      const authorEl = card.querySelector('[class*="author"], [class*="name"], [class*="user"]');
      const img = card.querySelector('img');
      return {
        noteId: noteIdFromHref(href),
        displayTitle: titleEl ? titleEl.textContent.trim() : '',
        user: { nickname: authorEl ? authorEl.textContent.trim() : '' },
        cover: { url: img ? (img.currentSrc || img.src || '') : '' },
        sourceUrl,
        xsecToken,
        xsecSource: xsecSource || 'pc_collect',
      };
    });
  }

  const stateNotes = notesFromState();
  const domNotes = notesFromDom();
  const seen = new Set();
  const notes = [];
  for (const note of [...stateNotes, ...domNotes]) {
    const noteId = noteIdOf(note);
    if (!noteId || seen.has(noteId)) continue;
    seen.add(noteId);
    notes.push(note);
  }
  window.__XHS_COLLECTION_DEBUG__ = {
    ...debug,
    returned: notes.length,
    stateCount: stateNotes.length,
    domCount: domNotes.length,
    scrollY: window.scrollY || document.documentElement.scrollTop || 0,
    innerHeight: window.innerHeight || 0,
    scrollHeight: document.documentElement.scrollHeight || document.body.scrollHeight || 0,
    atBottom: (window.scrollY || document.documentElement.scrollTop || 0) + (window.innerHeight || 0) >= ((document.documentElement.scrollHeight || document.body.scrollHeight || 0) - 12),
  };
  return notes;
})()
"#;

const XHS_FAVORITES_DEBUG_SCRIPT: &str = r#"
(() => window.__XHS_COLLECTION_DEBUG__ || {
  href: window.location.href,
  title: document.title || '',
  userKeys: [],
  candidatePaths: [],
  linkCount: document.querySelectorAll('a[href*="/explore/"], a[href*="/discovery/item/"]').length,
  cardCount: document.querySelectorAll('section.note-item, [class*="note-item"], [class*="note-card"]').length,
  stateCount: 0,
  domCount: 0,
  scrollY: window.scrollY || document.documentElement.scrollTop || 0,
  innerHeight: window.innerHeight || 0,
  scrollHeight: document.documentElement.scrollHeight || document.body.scrollHeight || 0,
  atBottom: (window.scrollY || document.documentElement.scrollTop || 0) + (window.innerHeight || 0) >= ((document.documentElement.scrollHeight || document.body.scrollHeight || 0) - 12),
  returned: 0
})()
"#;

const XHS_FAVORITES_API_HOOK_INSTALL_SCRIPT: &str = r#"
(() => {
  if (window.__XHS_COLLECTION_API_HOOK__ && window.__XHS_COLLECTION_API_HOOK__.installed) {
    return { installed: true, alreadyInstalled: true };
  }

  const hook = { installed: true, pages: [], errors: [] };
  window.__XHS_COLLECTION_API_HOOK__ = hook;

  function noteIdOf(note) {
    if (!note || typeof note !== 'object') return '';
    return String(note.note_id || note.noteId || note.id || (note.note_card && (note.note_card.note_id || note.note_card.id)) || '').trim();
  }

  function record(url, payload) {
    try {
      const textUrl = String(url || '');
      if (!textUrl.includes('/api/sns/web/v2/note/collect/page')) return;
      const data = payload && payload.data ? payload.data : payload;
      const notes = data && Array.isArray(data.notes) ? data.notes : [];
      if (!notes.length) return;
      hook.pages.push({
        url: textUrl,
        cursor: data.cursor || '',
        hasMore: Boolean(data.has_more),
        notes,
        first: noteIdOf(notes[0]),
        last: noteIdOf(notes[notes.length - 1]),
        capturedAt: Date.now(),
      });
      if (hook.pages.length > 200) hook.pages.splice(0, hook.pages.length - 200);
    } catch (error) {
      hook.errors.push(`record:${error && error.message ? error.message : String(error)}`);
    }
  }

  try {
    const originalFetch = window.fetch;
    if (originalFetch && !originalFetch.__xhsCollectionHooked) {
      const wrappedFetch = async function(...args) {
        const response = await originalFetch.apply(this, args);
        try {
          const url = typeof args[0] === 'string'
            ? args[0]
            : (args[0] && args[0].url) || '';
          if (String(url).includes('/api/sns/web/v2/note/collect/page')) {
            response.clone().json()
              .then(json => record(url, json))
              .catch(error => hook.errors.push(`fetch:${error && error.message ? error.message : String(error)}`));
          }
        } catch (error) {
          hook.errors.push(`fetch-wrap:${error && error.message ? error.message : String(error)}`);
        }
        return response;
      };
      wrappedFetch.__xhsCollectionHooked = true;
      window.fetch = wrappedFetch;
    }
  } catch (error) {
    hook.errors.push(`fetch-install:${error && error.message ? error.message : String(error)}`);
  }

  try {
    const originalOpen = XMLHttpRequest.prototype.open;
    const originalSend = XMLHttpRequest.prototype.send;
    if (!XMLHttpRequest.prototype.__xhsCollectionHooked) {
      XMLHttpRequest.prototype.open = function(method, url, ...rest) {
        this.__xhsCollectionUrl = url;
        return originalOpen.call(this, method, url, ...rest);
      };
      XMLHttpRequest.prototype.send = function(...args) {
        this.addEventListener('load', function() {
          try {
            const url = this.__xhsCollectionUrl || '';
            if (String(url).includes('/api/sns/web/v2/note/collect/page')) {
              record(url, JSON.parse(this.responseText || '{}'));
            }
          } catch (error) {
            hook.errors.push(`xhr:${error && error.message ? error.message : String(error)}`);
          }
        });
        return originalSend.apply(this, args);
      };
      XMLHttpRequest.prototype.__xhsCollectionHooked = true;
    }
  } catch (error) {
    hook.errors.push(`xhr-install:${error && error.message ? error.message : String(error)}`);
  }

  return { installed: true, alreadyInstalled: false };
})()
"#;

const XHS_FAVORITES_API_HOOK_DRAIN_SCRIPT: &str = r#"
(() => {
  const hook = window.__XHS_COLLECTION_API_HOOK__;
  const scrollY = window.scrollY || document.documentElement.scrollTop || 0;
  const scrollHeight = document.documentElement.scrollHeight || document.body.scrollHeight || 0;
  const atBottom = scrollY + (window.innerHeight || 0) >= (scrollHeight - 12);
  if (!hook) {
    return { installed: false, pages: [], errors: ['not_installed'], scrollY, scrollHeight, atBottom };
  }
  const pages = hook.pages.splice(0, hook.pages.length);
  const errors = hook.errors.splice(0, hook.errors.length);
  return { installed: true, pages, errors, scrollY, scrollHeight, atBottom };
})()
"#;

const XHS_FAVORITES_DISPLAY_COUNT_SCRIPT: &str = r#"
(() => {
  const texts = Array.from(document.querySelectorAll('[class*="tab"], [class*="feeds"], [role="tab"]'))
    .map(el => el.textContent || '')
    .join('\n');
  const bodyText = document.body ? document.body.innerText.slice(0, 1200) : '';
  const match = `${texts}\n${bodyText}`.match(/笔记\s*[・·•]\s*([\d,，]+)/);
  if (!match) return null;
  const count = Number(String(match[1]).replace(/[^\d]/g, ''));
  return Number.isFinite(count) && count > 0 ? count : null;
})()
"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalProfileSummary {
    id: String,
    display_name: String,
    source: Option<String>,
    source_account_id: Option<String>,
    avatar_url: Option<String>,
    db_path: String,
    media_dir: String,
    is_active: bool,
    session_status: String,
    last_opened_at: Option<String>,
    last_sync_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct LocalProfile {
    id: String,
    display_name: Option<String>,
    source: Option<String>,
    source_account_id: Option<String>,
    avatar_url: Option<String>,
    db_relative_path: String,
    media_relative_path: String,
    is_active: bool,
    session_status: String,
    last_opened_at: Option<String>,
    last_sync_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryOverview {
    app_data_dir: String,
    db_path: String,
    media_dir: String,
    notes_count: i64,
    media_count: i64,
    storage_root_id: String,
    active_profile: LocalProfileSummary,
    profiles: Vec<LocalProfileSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaAsset {
    id: String,
    note_id: String,
    media_type: String,
    download_status: String,
    original_url: Option<String>,
    relative_path: Option<String>,
    mime_type: Option<String>,
    size_bytes: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteSummary {
    id: String,
    source: String,
    source_note_id: String,
    source_url: String,
    title: String,
    excerpt: String,
    content: String,
    author_name: String,
    cover_url: Option<String>,
    note_type: String,
    published_at: Option<String>,
    collected_at: Option<String>,
    favorite_order: Option<i64>,
    last_synced_at: String,
    last_seen_at: Option<String>,
    remote_missing_at: Option<String>,
    remote_status: String,
    unavailable_reason: Option<String>,
    status: String,
    category_name: Option<String>,
    user_note: String,
    tags: Vec<String>,
    media: Vec<MediaAsset>,
}

#[derive(Debug)]
struct BaseNote {
    id: String,
    source: String,
    source_note_id: String,
    source_url: String,
    title: String,
    excerpt: String,
    content: String,
    author_name: String,
    cover_url: Option<String>,
    note_type: String,
    published_at: Option<String>,
    collected_at: Option<String>,
    favorite_order: Option<i64>,
    last_synced_at: String,
    last_seen_at: Option<String>,
    remote_missing_at: Option<String>,
    remote_status: String,
    unavailable_reason: Option<String>,
    status: String,
    category_name: Option<String>,
    user_note: String,
}

#[derive(Debug, Serialize)]
struct ImportSummary {
    inserted: usize,
    updated: usize,
    skipped: usize,
    #[serde(skip_serializing)]
    note_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct XhsFavoriteSyncInput {
    max_count: Option<usize>,
    resume: Option<bool>,
    full_sync: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchJobInput {
    limit: Option<usize>,
    asset_id: Option<String>,
    note_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchJobResult {
    scanned: usize,
    updated: usize,
    downloaded: usize,
    failed: usize,
    skipped: usize,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchJobProgress {
    phase: String,
    label: String,
    detail: String,
    planned: usize,
    scanned: usize,
    updated: usize,
    downloaded: usize,
    failed: usize,
    skipped: usize,
    progress: u8,
    indeterminate: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct XhsFavoriteSyncResult {
    scanned: usize,
    fetched: usize,
    inserted: usize,
    updated: usize,
    skipped: usize,
    existing_skipped: usize,
    remote_missing: usize,
    remote_display_count: Option<usize>,
    remote_unreturned_count: Option<usize>,
    limit_reached: bool,
    full_sync: bool,
    details_updated: usize,
    details_failed: usize,
    covers_downloaded: usize,
    covers_failed: usize,
    media_downloaded: usize,
    media_failed: usize,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct XhsSyncProgress {
    phase: String,
    label: String,
    detail: String,
    planned: usize,
    scanned: usize,
    fetched: usize,
    to_sync: Option<usize>,
    written: usize,
    inserted: usize,
    updated: usize,
    skipped: usize,
    existing_skipped: usize,
    progress: u8,
    indeterminate: bool,
}

struct XhsFavoriteCollectResult {
    notes: Vec<Value>,
    seen_note_ids: HashSet<String>,
    favorite_positions: Vec<(String, usize)>,
    scanned: usize,
    existing_skipped: usize,
    limit_reached: bool,
    reached_end: bool,
    stopped_reason: String,
    remote_display_count: Option<usize>,
    first_source_note_id: Option<String>,
    last_source_note_id: Option<String>,
}

#[derive(Debug)]
struct XhsSyncCheckpoint {
    anchor_source_note_id: Option<String>,
    reached_end: bool,
    scanned_count: usize,
    remote_display_count: Option<usize>,
}

#[derive(Debug, Default)]
struct XhsAccountInfo {
    user_id: Option<String>,
    nickname: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug)]
struct StoredXhsSession {
    source_account_id: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    session_key_id: Option<String>,
    session_storage: String,
    session_cookie: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct XhsSessionTestResult {
    ok: bool,
    status_code: u16,
    final_url: String,
    page_title: Option<String>,
    account_hint: Option<String>,
    account_id: Option<String>,
    account_name: Option<String>,
    avatar_url: Option<String>,
    cookie_keys: Vec<String>,
    checked_at: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct StatusUpdateInput {
    #[serde(rename = "noteId")]
    note_id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteMetadataUpdateInput {
    note_id: String,
    status: Option<String>,
    category_name: Option<String>,
    tags: Option<Vec<String>>,
    user_note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchNoteMetadataUpdateInput {
    note_ids: Vec<String>,
    status: Option<String>,
    category_name: Option<String>,
    add_tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportLibraryInput {
    format: String,
    include_media: Option<bool>,
    include_notes: Option<bool>,
    only_reviewed: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportLibraryResult {
    path: String,
    format: String,
    note_count: usize,
    media_count: usize,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiSettings {
    provider: String,
    base_url: String,
    model: String,
    has_api_key: bool,
    temperature: f64,
    max_tokens: i64,
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct AiRuntimeConfig {
    provider: String,
    base_url: String,
    model: String,
    api_key: String,
    temperature: f64,
    max_tokens: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiSettingsInput {
    provider: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
    clear_api_key: Option<bool>,
    temperature: Option<f64>,
    max_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiSettingsTestResult {
    ok: bool,
    message: String,
    model: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiClassifyInput {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiSplitCategoryInput {
    source_category_name: String,
    target_category_name: String,
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiTagGroupInput {
    limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiAssignmentResult {
    note_id: String,
    title: String,
    category_name: String,
    confidence: f64,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiClassificationResult {
    scanned: usize,
    updated: usize,
    created_categories: Vec<String>,
    assignments: Vec<AiAssignmentResult>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TagSummary {
    name: String,
    count: i64,
    kind: String,
    group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiTagAssignmentResult {
    tag: String,
    group_name: String,
    confidence: f64,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiTagGroupResult {
    scanned: usize,
    updated: usize,
    groups: Vec<String>,
    assignments: Vec<AiTagAssignmentResult>,
    message: String,
}

#[derive(Debug, Clone)]
struct AiNoteDigest {
    id: String,
    title: String,
    excerpt: String,
    content: String,
    author_name: String,
    category_name: Option<String>,
    tags: Vec<String>,
}

struct LibraryPaths {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    media_dir: PathBuf,
    profile: LocalProfile,
}

#[derive(Debug)]
struct NoteFetchTarget {
    id: String,
    source_note_id: String,
    source_url: String,
}

#[derive(Debug)]
struct MediaDownloadTarget {
    id: String,
    note_id: String,
    source_asset_id: Option<String>,
    note_source_note_id: String,
    media_type: String,
    original_url: String,
    mime_type: Option<String>,
}

#[derive(Debug)]
struct VideoStreamCandidate {
    url: String,
    codec: String,
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
    size_bytes: Option<i64>,
}

#[derive(Debug)]
enum XhsDetailFetchError {
    Gone(u16),
    Other(String),
}

#[derive(Debug)]
struct ExtractedMediaAsset {
    source_asset_id: String,
    media_type: String,
    original_url: String,
    mime_type: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
    size_bytes: Option<i64>,
}

#[tauri::command]
fn get_library_overview(app: AppHandle) -> Result<LibraryOverview, String> {
    let (paths, conn) = open_library(&app)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
fn list_local_profiles(app: AppHandle) -> Result<Vec<LocalProfileSummary>, String> {
    let app_data_dir = app_data_dir(&app)?;
    let (_, conn) = open_profile_registry(&app)?;
    read_local_profiles(&conn)
        .map(|profiles| profile_summaries(&app_data_dir, profiles))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn switch_local_profile(app: AppHandle, profile_id: String) -> Result<LibraryOverview, String> {
    let (_, conn) = open_profile_registry(&app)?;
    set_active_local_profile(&conn, &profile_id)
        .map_err(|error| format!("切换本地账号失败：{error}"))?;
    drop(conn);

    let (paths, conn) = open_library(&app)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
fn reset_library_data(app: AppHandle) -> Result<LibraryOverview, String> {
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
fn delete_library_database(app: AppHandle) -> Result<LibraryOverview, String> {
    let paths = resolve_paths(&app)?;
    if paths.db_path.exists() {
        fs::remove_file(&paths.db_path).map_err(|error| error.to_string())?;
    }
    let (paths, conn) = open_library(&app)?;
    library_overview(&app, &paths, &conn)
}

#[tauri::command]
fn clear_media_files(app: AppHandle) -> Result<LibraryOverview, String> {
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
fn list_notes(app: AppHandle) -> Result<Vec<NoteSummary>, String> {
    let (_, conn) = open_library(&app)?;
    read_notes(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_note_status(
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
fn update_note_metadata(
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
fn batch_update_note_metadata(
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
        Some(upsert_optional_category(&transaction, category_name).map_err(|error| error.to_string())?)
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
fn load_ai_settings(app: AppHandle) -> Result<AiSettings, String> {
    let (paths, conn) = open_library(&app)?;
    read_ai_settings(&conn, &paths)
}

#[tauri::command]
fn save_ai_settings(app: AppHandle, input: AiSettingsInput) -> Result<AiSettings, String> {
    let (paths, conn) = open_library(&app)?;
    save_ai_settings_with_conn(&conn, &paths, input)?;
    read_ai_settings(&conn, &paths)
}

#[tauri::command]
async fn test_ai_settings(app: AppHandle) -> Result<AiSettingsTestResult, String> {
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
fn list_tags(app: AppHandle) -> Result<Vec<TagSummary>, String> {
    let (_, conn) = open_library(&app)?;
    read_tag_summaries(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
async fn ai_classify_uncategorized(
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

    let original_categories: HashSet<String> = categories.iter().map(|name| name.to_lowercase()).collect();
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
async fn ai_split_category(
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
        read_ai_notes_in_category(&conn, &source_category, limit).map_err(|error| error.to_string())?
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
        let matched = filter_category_notes_with_ai(&config, &source_category, &target_category, &query, batch).await?;
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
async fn ai_group_tags(app: AppHandle, input: AiTagGroupInput) -> Result<AiTagGroupResult, String> {
    let config = {
        let (paths, conn) = open_library(&app)?;
        read_ai_runtime_config(&conn, &paths)?
    };
    let limit = input.limit.unwrap_or(AI_TAG_GROUP_DEFAULT_LIMIT).clamp(1, 600);
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

#[tauri::command]
fn export_library(
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
    let extension = if format == "markdown" { "md" } else { format.as_str() };
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

fn export_notes_markdown(notes: &[NoteSummary], include_media: bool, include_notes: bool) -> String {
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

#[tauri::command]
async fn load_xhs_saved_session(app: AppHandle) -> Result<Option<XhsSessionTestResult>, String> {
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
async fn enrich_xhs_note_details(
    app: AppHandle,
    input: BatchJobInput,
) -> Result<BatchJobResult, String> {
    let limit = input
        .limit
        .filter(|count| *count > 0)
        .unwrap_or(XHS_DETAIL_DEFAULT_LIMIT);
    let started_at = Instant::now();
    log::info!("xhs_detail_batch_start limit={limit}");

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
    let session = test_xhs_cookie(&cookie_header).await?;
    if !session.ok {
        return Err("当前小红书登录态不可用，请重新登录后再补全详情。".to_string());
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

        match fetch_xhs_note_detail(&client, &cookie_header, target).await {
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
async fn download_media_assets(
    app: AppHandle,
    input: BatchJobInput,
) -> Result<BatchJobResult, String> {
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
        return Ok(BatchJobResult {
            scanned: 0,
            updated: 0,
            downloaded: 0,
            failed: 0,
            skipped: 0,
            message: if requested_asset_id.is_some() {
                "这个媒体资产无需下载。".to_string()
            } else if requested_note_id.is_some() {
                "当前笔记没有需要下载的媒体资产。".to_string()
            } else {
                "没有需要下载的媒体资产。".to_string()
            },
        });
    }

    let cookie_header = read_current_or_saved_xhs_cookie(&app)?.unwrap_or_default();
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

    for (index, target) in targets.iter().enumerate() {
        let (_, conn) = open_library(&app)?;
        mark_media_asset_downloading(&conn, &target.id)
            .map_err(|error| format!("更新媒体下载状态失败：{error}"))?;
        drop(conn);

        emit_batch_progress(
            &app,
            "media-download-progress",
            BatchJobProgress {
                phase: "downloading".to_string(),
                label: "下载媒体".to_string(),
                detail: format!(
                    "正在下载第 {} / {planned} 个：{}",
                    index + 1,
                    target.note_source_note_id
                ),
                planned,
                scanned: result.scanned,
                updated: 0,
                downloaded: result.downloaded,
                failed: result.failed,
                skipped: result.skipped,
                progress: batch_progress_percent(result.scanned, planned).max(4),
                indeterminate: false,
            },
        );

        match download_media_bytes(&client, &cookie_header, target).await {
            Ok((bytes, response_mime)) => {
                let mime_type = response_mime.or_else(|| target.mime_type.clone());
                let relative_path = media_relative_path(target, mime_type.as_deref());
                let absolute_path = absolute_media_path(&paths.media_dir, &relative_path);
                if let Some(parent) = absolute_path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!(
                            "创建媒体目录失败 {}：{error}",
                            display_path(parent.to_path_buf())
                        )
                    })?;
                }
                fs::write(&absolute_path, &bytes).map_err(|error| {
                    format!(
                        "写入媒体文件失败 {}：{error}",
                        display_path(absolute_path.clone())
                    )
                })?;

                let (_, conn) = open_library(&app)?;
                mark_media_asset_downloaded(
                    &conn,
                    &target.id,
                    &relative_path,
                    bytes.len() as i64,
                    mime_type.as_deref(),
                )
                .map_err(|error| format!("记录媒体下载结果失败：{error}"))?;
                result.downloaded += 1;
            }
            Err(error) => {
                log::warn!(
                    "media_download_failed asset_id={} note_id={} error={}",
                    target.id,
                    target.note_source_note_id,
                    error
                );
                let (_, conn) = open_library(&app)?;
                mark_media_asset_failed(&conn, &target.id, &error)
                    .map_err(|db_error| format!("记录媒体下载失败状态失败：{db_error}"))?;
                result.failed += 1;
            }
        }

        result.scanned += 1;
        emit_batch_progress(
            &app,
            "media-download-progress",
            BatchJobProgress {
                phase: "downloading".to_string(),
                label: "下载媒体".to_string(),
                detail: format!(
                    "已处理 {} / {planned} 个，下载 {} 个，失败 {} 个。",
                    result.scanned, result.downloaded, result.failed
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

        if index + 1 < planned {
            std::thread::sleep(XHS_REQUEST_DELAY);
        }
    }

    result.message = format!(
        "媒体下载完成：处理 {} 个，下载 {} 个，失败 {} 个。耗时 {} 秒。",
        result.scanned,
        result.downloaded,
        result.failed,
        started_at.elapsed().as_secs()
    );
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
        match fetch_xhs_note_detail(&client, cookie_header, target).await {
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

    for (index, target) in targets.iter().enumerate() {
        let (_, conn) = open_library(app)?;
        mark_media_asset_downloading(&conn, &target.id)
            .map_err(|error| format!("更新媒体下载状态失败：{error}"))?;
        drop(conn);

        match download_media_bytes(&client, cookie_header, target).await {
            Ok((bytes, response_mime)) => {
                let mime_type = response_mime.or_else(|| target.mime_type.clone());
                let relative_path = media_relative_path(target, mime_type.as_deref());
                let absolute_path = absolute_media_path(&paths.media_dir, &relative_path);
                if let Some(parent) = absolute_path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!(
                            "创建媒体目录失败 {}：{error}",
                            display_path(parent.to_path_buf())
                        )
                    })?;
                }
                fs::write(&absolute_path, &bytes).map_err(|error| {
                    format!(
                        "写入媒体文件失败 {}：{error}",
                        display_path(absolute_path.clone())
                    )
                })?;

                let (_, conn) = open_library(app)?;
                mark_media_asset_downloaded(
                    &conn,
                    &target.id,
                    &relative_path,
                    bytes.len() as i64,
                    mime_type.as_deref(),
                )
                .map_err(|error| format!("记录媒体下载结果失败：{error}"))?;
                result.downloaded += 1;
            }
            Err(error) => {
                log::warn!(
                    "xhs_sync_media_download_failed asset_id={} note_id={} error={}",
                    target.id,
                    target.note_source_note_id,
                    error
                );
                let (_, conn) = open_library(app)?;
                mark_media_asset_failed(&conn, &target.id, &error)
                    .map_err(|db_error| format!("记录媒体下载失败状态失败：{db_error}"))?;
                result.failed += 1;
            }
        }

        result.scanned += 1;
        let progress = progress_start
            + (((index + 1) * progress_span as usize) / usize::max(planned, 1))
                .min(progress_span as usize) as u8;
        emit_xhs_sync_progress(
            app,
            XhsSyncProgress {
                phase: phase.to_string(),
                label: label.to_string(),
                detail: format!(
                    "已处理 {} / {planned} 个，下载 {} 个，失败 {} 个。",
                    result.scanned, result.downloaded, result.failed
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

        if index + 1 < planned {
            std::thread::sleep(XHS_REQUEST_DELAY);
        }
    }

    result.message = format!(
        "媒体下载：处理 {} 个，下载 {} 个，失败 {} 个。",
        result.scanned, result.downloaded, result.failed
    );
    Ok(result)
}

#[tauri::command]
async fn open_xhs_login_window(app: AppHandle) -> Result<(), String> {
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
async fn read_xhs_login_cookies(app: AppHandle) -> Result<XhsSessionTestResult, String> {
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
async fn test_xhs_session(app: AppHandle, cookie: String) -> Result<XhsSessionTestResult, String> {
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
async fn sync_xhs_favorites(
    app: AppHandle,
    input: XhsFavoriteSyncInput,
) -> Result<XhsFavoriteSyncResult, String> {
    let started_at = Instant::now();
    let requested_max = input.max_count.filter(|count| *count > 0);
    let resume = input.resume.unwrap_or(false);
    let full_sync = input.full_sync.unwrap_or(false) || resume;
    let planned_count = requested_max.unwrap_or(0);
    let plan_detail = requested_max.map_or_else(
        || {
            if full_sync {
                if resume {
                    "从当前收藏页位置继续完整同步，直到收藏页末尾。".to_string()
                } else {
                    "完整同步会读取到收藏页末尾，并校准远端缺失状态。".to_string()
                }
            } else {
                "快速同步只读取最近收藏；遇到已同步笔记或收藏总数未变化就停止。".to_string()
            }
        },
        |count| format!("本次最多读取 {count} 条收藏。"),
    );
    log::info!(
        "xhs_native_sync_start requested_max={:?} resume={} full_sync={} max_scroll_attempts={}",
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
    let (cookie_header, _) = read_xhs_cookie_header(window.clone())?;
    log::info!(
        "xhs_native_sync_cookie_read_done key_count={}",
        cookie_keys(&cookie_header).len()
    );

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
    if let Ok(account) = extract_xhs_account_info(&window) {
        merge_account_info_into_session_result(&mut session, account);
    }
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
    log::info!("xhs_native_sync_user_detect_done");
    let (_, conn) = open_library(&app)?;
    let existing_note_ids = read_xhs_existing_note_ids(&conn)
        .map_err(|error| format!("读取本地收藏索引失败：{error}"))?;
    let checkpoint = read_xhs_sync_checkpoint(&conn, &user_id)
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
            label: "读取收藏列表".to_string(),
            detail: if resume {
                "正在从当前收藏页位置继续读取收藏卡片。".to_string()
            } else if full_sync {
                "正在完整读取收藏页，直到页面末尾。".to_string()
            } else {
                "正在快速读取最近收藏，遇到已同步笔记就停止。".to_string()
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
        &raw_notes,
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
    let remote_missing = if full_sync
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
    upsert_xhs_sync_checkpoint(&conn, &user_id, &collect_result)
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
        let result = run_xhs_post_sync_background(
            &app,
            &cookie_header,
            note_ids,
            scanned,
            existing_skipped,
        )
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
            detail: format!(
                "索引已写入，正在后台补全 {planned} 条收藏的正文、封面和首批媒体。"
            ),
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
    let cover_targets = read_media_download_targets_filtered(
        &conn,
        usize::MAX,
        &["cover"],
        Some(&note_id_set),
    )
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

fn collect_xhs_favorites<R: Runtime>(
    app: &AppHandle,
    window: &WebviewWindow<R>,
    user_id: &str,
    requested_max: Option<usize>,
    resume: bool,
    full_sync: bool,
    existing_note_ids: &HashSet<String>,
    checkpoint: Option<&XhsSyncCheckpoint>,
) -> Result<XhsFavoriteCollectResult, String> {
    let profile_url =
        format!("https://www.xiaohongshu.com/user/profile/{user_id}?tab=fav&subTab=note");
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
        let last = page
            .get("last")
            .and_then(Value::as_str)
            .unwrap_or_default();
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
    limit: usize,
) -> rusqlite::Result<Vec<NoteFetchTarget>> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.source_note_id, n.source_url
         FROM notes n
         WHERE n.source = 'xhs'
           AND n.remote_status = 'available'
           AND COALESCE(n.source_url, '') <> ''
           AND (
             COALESCE(n.content, '') = ''
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
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(NoteFetchTarget {
            id: row.get(0)?,
            source_note_id: row.get(1)?,
            source_url: row.get(2)?,
        })
    })?;
    rows.collect()
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
             ELSE 3
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
             ELSE 3
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
             ELSE 3
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

async fn fetch_xhs_note_detail(
    client: &Client,
    cookie_header: &str,
    target: &NoteFetchTarget,
) -> Result<Value, XhsDetailFetchError> {
    log::info!(
        "xhs_detail_fetch_start note_id={} url={}",
        target.source_note_id,
        target.source_url
    );
    let response = client
        .get(&target.source_url)
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

    for tag_name in xhs_detail_tags(detail) {
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

fn xhs_detail_tags(value: &Value) -> Vec<String> {
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
                    if !name.is_empty() && seen.insert(name.clone()) {
                        tags.push(name);
                    }
                }
            }
        }
    }
    tags
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
    let directory = if target.media_type == "video" {
        "videos"
    } else {
        "images"
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

fn absolute_media_path(media_dir: &PathBuf, relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .fold(media_dir.clone(), |path, segment| path.join(segment))
}

fn media_file_extension(media_type: &str, mime_type: Option<&str>, url: &str) -> &'static str {
    let mime = mime_type.unwrap_or_default().to_lowercase();
    let lower_url = url.to_lowercase();
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
    let stored = conn.query_row(
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

fn read_current_or_saved_xhs_cookie(app: &AppHandle) -> Result<Option<String>, String> {
    if let Some(window) = app.get_webview_window(XHS_LOGIN_WINDOW_LABEL) {
        if let Ok((cookie, _)) = read_xhs_cookie_header(window) {
            if !cookie.trim().is_empty() {
                return Ok(Some(cookie));
            }
        }
    }

    let (_, conn) = open_library(app)?;
    read_latest_xhs_stored_session(&conn)
        .map(|stored| stored.map(|session| session.session_cookie))
        .map_err(|error| error.to_string())
}

fn script_with_unwrap(script: &str) -> String {
    script.replace("__UNWRAP_JS__", XHS_UNWRAP_JS)
}

fn open_library(app: &AppHandle) -> Result<(LibraryPaths, Connection), String> {
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

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    Ok(app_data_dir)
}

fn library_overview(
    app: &AppHandle,
    paths: &LibraryPaths,
    conn: &Connection,
) -> Result<LibraryOverview, String> {
    let notes_count = count_rows(conn, "notes")?;
    let media_count = count_rows(conn, "media_assets")?;
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
        storage_root_id: STORAGE_ROOT_ID.to_string(),
        active_profile,
        profiles: summaries,
    })
}

fn resolve_paths(app: &AppHandle) -> Result<LibraryPaths, String> {
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

fn open_profile_registry(app: &AppHandle) -> Result<(PathBuf, Connection), String> {
    let app_data_dir = app_data_dir(app)?;
    let db_path = app_data_dir.join(PROFILE_REGISTRY_DB);
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    ensure_profile_registry(&conn, &app_data_dir).map_err(|error| error.to_string())?;
    Ok((db_path, conn))
}

fn ensure_profile_registry(conn: &Connection, app_data_dir: &Path) -> rusqlite::Result<()> {
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
        let legacy_db = app_data_dir.join("library.sqlite");
        let (db_relative_path, media_relative_path) = if legacy_db.exists() {
            ("library.sqlite".to_string(), "media".to_string())
        } else {
            ("library.sqlite".to_string(), "media".to_string())
        };
        conn.execute(
            "INSERT INTO local_profiles (
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

fn read_local_profiles(conn: &Connection) -> rusqlite::Result<Vec<LocalProfile>> {
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

fn read_active_local_profile(conn: &Connection) -> rusqlite::Result<LocalProfile> {
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

fn find_local_profile_by_xhs_id(
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

fn local_profile_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalProfile> {
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

fn profile_summaries(app_data_dir: &Path, profiles: Vec<LocalProfile>) -> Vec<LocalProfileSummary> {
    profiles
        .iter()
        .map(|profile| local_profile_summary(app_data_dir, profile))
        .collect()
}

fn local_profile_summary(app_data_dir: &Path, profile: &LocalProfile) -> LocalProfileSummary {
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

fn set_active_local_profile(conn: &Connection, profile_id: &str) -> rusqlite::Result<()> {
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

    let active_unbound = active.source_account_id.as_deref().unwrap_or_default().is_empty();
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

fn ensure_storage_root(conn: &Connection, paths: &LibraryPaths) -> Result<(), String> {
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

fn ensure_schema_upgrades(conn: &Connection) -> rusqlite::Result<()> {
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
    add_column_if_missing(conn, "tags", "ai_group", "ai_group TEXT")?;
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
    Ok(())
}

fn add_column_if_missing(
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

fn count_rows(conn: &Connection, table: &str) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn read_notes(conn: &Connection) -> rusqlite::Result<Vec<NoteSummary>> {
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

fn read_tags(conn: &Connection, note_id: &str) -> rusqlite::Result<Vec<String>> {
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

fn read_media(conn: &Connection, note_id: &str) -> rusqlite::Result<Vec<MediaAsset>> {
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

fn read_xhs_existing_note_ids(conn: &Connection) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT source_note_id FROM notes WHERE source = 'xhs'")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn read_xhs_sync_checkpoint(
    conn: &Connection,
    user_id: &str,
) -> rusqlite::Result<Option<XhsSyncCheckpoint>> {
    conn.query_row(
        "SELECT anchor_source_note_id, reached_end, scanned_count, remote_display_count
         FROM sync_checkpoints
         WHERE source = 'xhs' AND source_account_id = ?1 AND mode = 'favorites'",
        params![user_id],
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
    collect_result: &XhsFavoriteCollectResult,
) -> rusqlite::Result<()> {
    if !collect_result.limit_reached
        && !collect_result.reached_end
        && collect_result.remote_display_count.is_none()
    {
        return Ok(());
    }

    let checkpoint_id = format!("xhs:{user_id}:favorites");
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
         VALUES (?1, 'xhs', ?2, 'favorites', ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP, ?7)
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
                     excerpt = ?3,
                     author_name = ?4,
                     cover_url = ?5,
                     note_type = ?6,
                     published_at = COALESCE(?7, published_at),
                     collected_at = COALESCE(?8, collected_at),
                     raw_json = ?9,
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

fn upsert_tag_with_kind(conn: &Connection, name: &str, kind: &str) -> rusqlite::Result<String> {
    let id = format!("tag:{name}");
    conn.execute(
        "INSERT INTO tags (id, name, kind)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET
            kind = CASE WHEN tags.kind = 'user' THEN tags.kind ELSE excluded.kind END,
            updated_at = CURRENT_TIMESTAMP",
        params![id, name, kind],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )?)
}

fn upsert_optional_category(conn: &Connection, name: &str) -> rusqlite::Result<Option<String>> {
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

fn replace_user_tags_for_note(
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

fn normalize_tag_names(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for raw in values {
        for part in raw.split(|ch: char| matches!(ch, ',' | '，' | ';' | '；' | '\n' | '\t')) {
            let tag = part.trim().trim_start_matches('#').trim();
            if tag.is_empty() || tag.len() > 64 {
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

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

fn ai_api_key_id(profile_id: &str) -> String {
    let safe_profile_id: String = profile_id
        .chars()
        .map(|ch| if matches!(ch, '\\' | '/' | ':') { '_' } else { ch })
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
            "SELECT provider, base_url, model, api_key_key_id, api_key_fallback,
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
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let Some((provider, base_url, model, key_id, fallback, temperature, max_tokens, updated_at)) =
        row
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
    let has_api_key =
        !fallback.trim().is_empty() || (!key_id.trim().is_empty() && read_ai_secret(&key_id).is_ok());

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
        storage = "none".to_string();
        fallback.clear();
    }

    if let Some(api_key) = input.api_key.as_deref().map(str::trim).filter(|value| !value.is_empty())
    {
        let next_key_id = key_id.clone().unwrap_or_else(|| ai_api_key_id(&paths.profile.id));
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

fn read_ai_runtime_config(conn: &Connection, paths: &LibraryPaths) -> Result<AiRuntimeConfig, String> {
    let (provider, base_url, model, key_id, fallback, temperature, max_tokens) = conn
        .query_row(
            "SELECT provider, base_url, model, api_key_key_id, api_key_fallback,
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
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "请先在设置里保存 AI API 配置。".to_string())?;

    let key_id = key_id.unwrap_or_else(|| ai_api_key_id(&paths.profile.id));
    let api_key = match read_ai_secret(&key_id) {
        Ok(secret) if !secret.trim().is_empty() => secret,
        Ok(_) | Err(_) if !fallback.trim().is_empty() => fallback,
        Ok(_) => return Err("AI API Key 为空，请在设置里填写。".to_string()),
        Err(error) => {
            log::warn!("ai_keyring_read_failed key_id={key_id} error={error}");
            return Err("AI API Key 读取失败，请在设置里重新保存。".to_string());
        }
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
            truncate_chars(text.trim(), 180)
        )
    })
}

async fn call_ai_text(config: &AiRuntimeConfig, system: &str, user: &str) -> Result<String, String> {
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
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("AI 响应 JSON 解析失败：{error}"))?;
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("AI 响应缺少 message.content：{}", truncate_chars(&text, 260)))
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
        .ok_or_else(|| format!("Claude 响应缺少 content.text：{}", truncate_chars(&text, 260)))
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

fn read_tag_summaries_limited(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<TagSummary>> {
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
        let raw_category = fixed_category
            .map(str::to_string)
            .or_else(|| item.get("categoryName").and_then(Value::as_str).map(str::to_string));
        let Some(category_name) = raw_category.as_deref().and_then(sanitize_category_name) else {
            continue;
        };
        assignments.push(AiAssignmentResult {
            note_id: note_id.to_string(),
            title: note_titles.get(note_id).copied().unwrap_or_default().to_string(),
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
            title: note_titles.get(note_id).copied().unwrap_or_default().to_string(),
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
        .map(|ch| if matches!(ch, '\r' | '\n' | '\t') { ' ' } else { ch })
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
    if value.starts_with("https://www.xiaohongshu.com/") {
        return Some(value.to_string());
    }
    if value.starts_with('/') {
        return Some(format!("https://www.xiaohongshu.com{value}"));
    }
    None
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

fn display_path(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

fn main_navigation_guard<R: Runtime>() -> TauriPlugin<R> {
    PluginBuilder::new("main-navigation-guard")
        .on_navigation(|webview, url| {
            if webview.label() != MAIN_WINDOW_LABEL {
                return true;
            }

            let allowed = is_main_window_url(url);
            if !allowed {
                log::warn!("main_window_external_navigation_blocked url={url}");
            }
            allowed
        })
        .build()
}

fn is_main_window_url(url: &Url) -> bool {
    match url.scheme() {
        "about" | "data" | "tauri" | "asset" => true,
        "http" | "https" => matches!(
            url.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("tauri.localhost")
        ),
        _ => false,
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(main_navigation_guard())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_library_overview,
            list_local_profiles,
            switch_local_profile,
            delete_library_database,
            clear_media_files,
            reset_library_data,
            list_notes,
            update_note_status,
            update_note_metadata,
            batch_update_note_metadata,
            load_ai_settings,
            save_ai_settings,
            test_ai_settings,
            list_tags,
            ai_classify_uncategorized,
            ai_split_category,
            ai_group_tags,
            export_library,
            load_xhs_saved_session,
            open_xhs_login_window,
            read_xhs_login_cookies,
            test_xhs_session,
            sync_xhs_favorites,
            enrich_xhs_note_details,
            download_media_assets
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
