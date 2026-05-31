# Release Notes

## v0.1.1 - Desktop Polish And AI Tag Governance

Release date: 2026-05-31

XHS Collection 0.1.1 focuses on making the preview build easier to use day to day: better diagnostics, stronger AI categorization controls, cleaner tag management, richer media/detail workflows, and a refreshed visual identity.

### Highlights

- Refreshed app logo, favicon, and platform icon set across Windows, macOS, Linux, iOS, and Android icon outputs.
- Windows release builds now hide the extra console window while keeping logs on disk.
- Added per-launch log files under the app log directory, plus `latest.log` generation on exit.
- Added Settings entries to open the current log/latest log and clear historical logs from Danger Zone.
- Added editable AI prompts in Settings. Custom prompts are saved under the app data directory, with bundled defaults compiled into the desktop backend.
- Added AI tag governance: noise/timecode tag cleanup, rule-based and AI-assisted merge suggestions, alias creation, and apply/review flow.
- Added clearer AI progress/error states, cancellation support, smaller retry batches, and better OpenAI/OpenRouter GPT-5 reasoning response handling.
- Added library content coverage stats and a full “continue detail enrichment” flow without the previous 30-item cap.
- Added experimental Xiaohongshu albums/files sync commands and UI entry points.
- Improved media/detail handling for covers, thumbnails, multi-image notes, file assets, video preview, and on-demand media download.
- Added whole-library Zip backup alongside JSON/CSV/Markdown export.
- Improved desktop UI polish: collapsible sidebar alignment, circular collapsed account avatar, resizable library detail panel, progressive rendering for large library/media lists, and simplified library quick stats.

### Fixes And Improvements

- Fixed the post-sync loading state so sync completion no longer leaves the UI spinning.
- Fixed AI category split behavior so it uses model analysis instead of plain text search.
- Fixed tag cleanup UI refresh and disabled overlapping AI actions while a governance job is running.
- Fixed generated tag counts and tag grouping behavior after detail enrichment.
- Fixed session/account display issues by retaining local profile metadata and showing the active account more consistently.
- Fixed downloaded media preview paths so local videos/images can render through Tauri's asset protocol.
- Improved sync cancellation reporting for favorites, detail enrichment, media downloads, albums, and files.

### Privacy And Storage

- Session cookies and AI API keys still prefer the OS credential store/keychain.
- Logs are written locally and are intended for troubleshooting. Users can inspect or clear them from Settings.
- AI features send only the selected metadata/content snippets to the provider configured by the user.
- Library backups are local Zip files created under the app data directory.

### Known Limitations

- Xiaohongshu album/file sync is still experimental and depends on current web UI structure.
- Very large media backups use a simple stored Zip writer and can hit Zip32 size/count limits.
- AI governance quality depends on the selected model following structured JSON output.
- macOS builds are not yet signed or notarized.
- Windows builds are not yet code-signed.
- Automatic updates are not enabled.

### Verification

Before preparing this PR, the project was verified with:

```bash
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

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
