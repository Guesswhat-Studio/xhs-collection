# Release Notes

## v0.1.0 - Technical Preview

Release date: 2026-05-31

XHS Collection 0.1.0 is the first desktop preview release. The goal of this version is to validate the local-first collection workflow: connect an account, sync favorites, enrich notes, download selected media, organize locally, and test AI-assisted classification.

### Highlights

- New Tauri v2 desktop app for Windows, macOS Apple Silicon, and Linux.
- Local SQLite library with note IDs used for deduplication.
- Xiaohongshu login window, session validation, saved local login state, and manual cookie fallback.
- Fast sync and full sync modes for favorites.
- Incremental scan strategy that can stop after reaching already-known items.
- Local multi-account profile registry, with one local profile bound to one Xiaohongshu account.
- Note library with grid/list views, status filters, search, sorting by collected time, and adjustable detail panel.
- Note metadata editing: status, one primary category, multiple tags, and user notes.
- Category and tag dashboard with AI-assisted uncategorized classification, category split, and tag grouping.
- Media library for covers, images, and videos, with local preview and file reveal/open actions.
- First 30 media assets can be auto-downloaded after sync; the rest can be downloaded on demand.
- Export to JSON, CSV, and Markdown.
- AI provider settings with OpenAI-compatible endpoints, Claude, OpenRouter, DeepSeek, SiliconFlow, Qwen, Kimi, MiniMax, GLM, Hunyuan, and custom endpoint support.
- GitHub Actions workflow for Windows, macOS ARM, and Linux builds.

### Privacy And Storage

- Library data is stored locally in SQLite.
- Media files are stored under the app data directory for each operating system.
- Xiaohongshu session cookies and AI API keys prefer the operating system's credential store/keychain; SQLite fallback is used only when secure storage is unavailable.
- AI classification sends selected note titles, excerpts/content, authors, categories, and tags to the provider configured by the user.

### Known Limitations

- This is an unofficial tool and depends on web/session behavior that Xiaohongshu may change.
- macOS builds are not yet signed or notarized.
- Windows builds are not yet code-signed.
- Linux builds may require distribution-specific WebKitGTK dependencies.
- Details/media enrichment can fail for deleted, unavailable, private, or changed posts.
- Media downloads are intentionally conservative; large batch downloading still needs a stronger queue, retry, and rate-limit UI.
- Albums/folders from Xiaohongshu are not fully mapped yet.
- Automatic updates are not enabled.

### Upgrade Notes

- 0.1.0 is the first public preview, so no migration from a public release is required.
- If you used an earlier local development build, keep a backup of the app data directory before switching profiles or clearing local data.

### Verification

Before tagging this release locally, the project was verified with:

```bash
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri:build
```

### Release Checklist

- Push a version tag such as `v0.1.0`.
- Wait for GitHub Actions to build Windows, macOS ARM, and Linux artifacts.
- Review the generated draft release.
- Add platform-specific installation notes if CI surfaces signing or dependency warnings.
- Publish only after a second-account smoke test passes.
