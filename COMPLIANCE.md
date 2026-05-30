# Compliance And Responsible Use

XHS Collection is a local-first desktop tool for organizing a user's own Xiaohongshu / Rednote favorites. It is not an official Xiaohongshu client and is not affiliated with, endorsed by, sponsored by, or approved by Xiaohongshu / Rednote.

This document is a practical compliance guide for users and contributors. It is not legal advice.

## Intended Use

Use XHS Collection to:

- Sync favorites from an account you own or are authorized to access.
- Keep a personal local index of saved notes.
- Download media only when you have the right to store it for personal use.
- Add local categories, tags, status, and notes.
- Export your own local metadata for backup or migration.

## User Responsibilities

Users are responsible for:

- Following Xiaohongshu / Rednote terms, community rules, copyright rules, and local laws.
- Respecting creators' rights and not redistributing downloaded media without permission.
- Keeping account cookies, local database files, media folders, and API keys secure.
- Configuring AI providers in a way that matches their own privacy and data-processing requirements.
- Stopping use if a platform asks them to stop or if a feature becomes incompatible with platform rules.

## Prohibited Use

Do not use XHS Collection to:

- Access accounts, content, or media you are not authorized to access.
- Bypass paywalls, privacy controls, login restrictions, CAPTCHAs, security checks, or rate limits.
- Mass-harvest, resell, republish, or redistribute creator content.
- Build datasets containing personal data without appropriate rights and consent.
- Spam, manipulate engagement, automate account actions, or modify remote content.
- Share session cookies, credential-store entries, local databases, or downloaded media publicly.

## Platform Interaction

The app is designed as a read-only local organizer:

- Sync reads the user's own favorites and writes to local storage.
- The app does not write back to Xiaohongshu, edit posts, like, comment, follow, or message users.
- Session cookies are used only to validate access and read pages/API responses available to the signed-in user.
- Deleted, unavailable, private, or changed remote posts may be marked as missing locally.

The app may stop working if Xiaohongshu / Rednote changes its web app, authentication behavior, APIs, or anti-abuse systems.

## Copyright And Content Rights

The MIT license in this repository applies only to the XHS Collection source code and project assets created for this repository. It does not grant rights to:

- Xiaohongshu / Rednote trademarks, logos, UI, or platform content.
- Creator photos, videos, text, comments, music, or other media.
- Any third-party model, API, website, or service.

Downloaded media remains subject to the rights and licenses of the original creators and platforms.

## Privacy And Local Data

By default, XHS Collection stores data locally:

- Notes, categories, tags, sync runs, and metadata are stored in SQLite.
- Media files are stored under the operating system's app data directory.
- Xiaohongshu session cookies prefer the system credential store/keychain.
- AI API keys prefer the system credential store/keychain.
- If secure storage is unavailable, the app may fall back to local SQLite storage.

Users should avoid committing, uploading, or sharing local app data directories, database files, media files, logs, or screenshots that contain personal information.

## AI Providers

AI classification is optional. When enabled, the app sends selected note data to the provider configured by the user. Depending on the operation, this may include:

- Note title
- Author name
- Excerpt or text content
- Existing category
- Tags
- User-provided split/filter prompt

Before enabling AI features, review the chosen provider's terms, privacy policy, retention policy, and data processing settings. Do not send sensitive, private, regulated, or third-party-confidential content to a provider unless you have the right to do so.

## Security Notes

- Never paste cookies or API keys into issue reports.
- Remove personal paths, profile IDs, cookies, note URLs, and downloaded media paths from logs before sharing.
- Report security concerns through a private channel if available; otherwise, open a minimal GitHub issue without credentials or personal data.

## Distribution Notes

0.1.0 is a technical preview:

- Builds may be unsigned.
- macOS builds may require manual approval by the user because notarization is not configured yet.
- Windows builds may show SmartScreen warnings until code signing is added.
- Users should download releases only from the official GitHub repository.

## Contributor Guidelines

Contributors should:

- Keep the app read-only with respect to Xiaohongshu unless a future feature is explicitly reviewed.
- Avoid committing private planning docs, local data, cookies, logs, screenshots with real user data, or build artifacts.
- Prefer conservative sync/download behavior over aggressive scraping.
- Document changes that affect privacy, storage, authentication, AI data flow, or platform interaction.
