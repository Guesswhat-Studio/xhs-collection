PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  source TEXT NOT NULL,
  source_account_id TEXT,
  display_name TEXT,
  avatar_url TEXT,
  session_status TEXT NOT NULL DEFAULT 'unknown',
  session_cookie TEXT,
  session_key_id TEXT,
  session_storage TEXT NOT NULL DEFAULT 'sqlite',
  session_checked_at TEXT,
  last_login_at TEXT,
  last_sync_at TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (source, source_account_id)
);

CREATE TABLE IF NOT EXISTS storage_roots (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  platform TEXT NOT NULL DEFAULT 'all',
  base_dir TEXT NOT NULL,
  relative_path TEXT NOT NULL DEFAULT '',
  absolute_path TEXT,
  is_active INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY,
  source TEXT NOT NULL DEFAULT 'xhs',
  source_note_id TEXT NOT NULL,
  source_url TEXT NOT NULL,
  title TEXT,
  excerpt TEXT,
  content TEXT,
  author_id TEXT,
  author_name TEXT,
  cover_url TEXT,
  note_type TEXT NOT NULL DEFAULT 'unknown',
  published_at TEXT,
  collected_at TEXT,
  favorite_order INTEGER,
  first_synced_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_synced_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_seen_at TEXT,
  remote_updated_at TEXT,
  remote_missing_at TEXT,
  remote_status TEXT NOT NULL DEFAULT 'available',
  unavailable_reason TEXT,
  status TEXT NOT NULL DEFAULT 'unread',
  category_id TEXT,
  user_note TEXT NOT NULL DEFAULT '',
  archived_at TEXT,
  raw_json TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (source, source_note_id),
  FOREIGN KEY (category_id) REFERENCES categories(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS categories (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  parent_id TEXT,
  color TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (name, parent_id),
  FOREIGN KEY (parent_id) REFERENCES categories(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL DEFAULT 'user',
  ai_group TEXT,
  color TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS tag_aliases (
  id TEXT PRIMARY KEY,
  alias_name TEXT NOT NULL,
  normalized_alias TEXT NOT NULL UNIQUE,
  canonical_tag_id TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'manual',
  confidence REAL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (canonical_tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ai_settings (
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
);

CREATE TABLE IF NOT EXISTS note_tags (
  note_id TEXT NOT NULL,
  tag_id TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (note_id, tag_id),
  FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
  FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS albums (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT 'local',
  source_album_id TEXT,
  source_account_id TEXT,
  source_url TEXT,
  cover_url TEXT,
  note_count INTEGER,
  raw_json TEXT,
  last_synced_at TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS album_notes (
  album_id TEXT NOT NULL,
  note_id TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (album_id, note_id),
  FOREIGN KEY (album_id) REFERENCES albums(id) ON DELETE CASCADE,
  FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS media_assets (
  id TEXT PRIMARY KEY,
  note_id TEXT NOT NULL,
  source TEXT NOT NULL DEFAULT 'xhs',
  source_asset_id TEXT,
  media_type TEXT NOT NULL,
  original_url TEXT,
  storage_root_id TEXT,
  relative_path TEXT,
  mime_type TEXT,
  width INTEGER,
  height INTEGER,
  duration_ms INTEGER,
  size_bytes INTEGER,
  sha256 TEXT,
  download_status TEXT NOT NULL DEFAULT 'not_downloaded',
  download_error TEXT,
  downloaded_at TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (note_id, source_asset_id),
  FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE,
  FOREIGN KEY (storage_root_id) REFERENCES storage_roots(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS sync_runs (
  id TEXT PRIMARY KEY,
  account_id TEXT,
  source TEXT NOT NULL DEFAULT 'xhs',
  status TEXT NOT NULL,
  started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  finished_at TEXT,
  fetched_count INTEGER NOT NULL DEFAULT 0,
  scanned_count INTEGER NOT NULL DEFAULT 0,
  inserted_count INTEGER NOT NULL DEFAULT 0,
  updated_count INTEGER NOT NULL DEFAULT 0,
  skipped_count INTEGER NOT NULL DEFAULT 0,
  existing_skipped_count INTEGER NOT NULL DEFAULT 0,
  remote_missing_count INTEGER NOT NULL DEFAULT 0,
  reached_end INTEGER NOT NULL DEFAULT 0,
  limit_reached INTEGER NOT NULL DEFAULT 0,
  stop_reason TEXT,
  first_source_note_id TEXT,
  last_source_note_id TEXT,
  error_message TEXT,
  FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS sync_checkpoints (
  id TEXT PRIMARY KEY,
  source TEXT NOT NULL DEFAULT 'xhs',
  source_account_id TEXT,
  mode TEXT NOT NULL,
  anchor_source_note_id TEXT,
  reached_end INTEGER NOT NULL DEFAULT 0,
  scanned_count INTEGER NOT NULL DEFAULT 0,
  last_success_at TEXT,
  stop_reason TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (source, source_account_id, mode)
);

CREATE TABLE IF NOT EXISTS sync_run_items (
  sync_run_id TEXT NOT NULL,
  note_id TEXT,
  source_note_id TEXT NOT NULL,
  action TEXT NOT NULL,
  error_message TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (sync_run_id, source_note_id),
  FOREIGN KEY (sync_run_id) REFERENCES sync_runs(id) ON DELETE CASCADE,
  FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_status ON notes(status);
CREATE INDEX IF NOT EXISTS idx_notes_category_id ON notes(category_id);
CREATE INDEX IF NOT EXISTS idx_notes_last_synced_at ON notes(last_synced_at);
CREATE INDEX IF NOT EXISTS idx_notes_remote_status ON notes(remote_status);
CREATE INDEX IF NOT EXISTS idx_notes_collected_at ON notes(collected_at);
CREATE INDEX IF NOT EXISTS idx_notes_favorite_order ON notes(favorite_order);
CREATE INDEX IF NOT EXISTS idx_albums_source ON albums(source, source_account_id, source_album_id);
CREATE INDEX IF NOT EXISTS idx_album_notes_note_id ON album_notes(note_id);
CREATE INDEX IF NOT EXISTS idx_media_assets_note_id ON media_assets(note_id);
CREATE INDEX IF NOT EXISTS idx_media_assets_download_status ON media_assets(download_status);
CREATE INDEX IF NOT EXISTS idx_storage_roots_kind ON storage_roots(kind);
CREATE INDEX IF NOT EXISTS idx_sync_checkpoints_source ON sync_checkpoints(source, source_account_id, mode);
