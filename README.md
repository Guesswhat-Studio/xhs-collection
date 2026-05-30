<p align="center">
  <img src="assets/brand/xhs-collection-logo.png" width="96" height="96" alt="XHS Collection logo" />
</p>

<h1 align="center">XHS Collection</h1>

<p align="center">
  一个面向小红书收藏的本地优先桌面整理台。同步收藏、下载媒体、复查状态、分类标签和 AI 自动整理，都放在自己的电脑里。
</p>

<p align="center">
  <a href="https://github.com/Guesswhat-Studio/xhs-collection/actions/workflows/release.yml"><img alt="Desktop CI" src="https://github.com/Guesswhat-Studio/xhs-collection/actions/workflows/release.yml/badge.svg" /></a>
  <img alt="Version" src="https://img.shields.io/badge/version-0.1.0-e9364f" />
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-v2-24c8db" />
  <img alt="Local first" src="https://img.shields.io/badge/local--first-SQLite-2d8f6f" />
</p>

> XHS Collection 是非官方工具，与小红书 / Rednote 没有关联。请只同步和下载你自己账号有权访问的内容，并遵守对应平台条款。

## Preview

![收藏库预览](assets/screenshots/library-preview.svg)

![AI 分类预览](assets/screenshots/ai-preview.svg)

## Highlights

- **小红书收藏同步**：通过内置登录窗口读取本机登录态，按笔记 ID 去重，同步到本地 SQLite。
- **本地复查工作台**：收藏按最近收藏排序，支持待看、已看、过时、归档等状态。
- **分类和标签**：每条收藏有一个主分类，标签可承载多个横向维度；新分类输入后自动创建。
- **图片 / 视频管理**：媒体下载到系统对应的应用数据目录，收藏库和媒体库里可直接预览。
- **多本地账号**：本地账号绑定唯一小红书账号，后续同步会自动匹配账号。
- **AI 自动整理**：支持 OpenAI 兼容接口、Claude，以及 DeepSeek、硅基流动、通义千问、Kimi、MiniMax、智谱 GLM、腾讯混元、OpenRouter 和自定义端点。
- **导出**：收藏可导出为 JSON、CSV、Markdown，方便迁移或二次处理。
- **跨平台桌面端**：Tauri v2 + React + Rust，目标平台包括 Windows、macOS Apple Silicon、Linux。

## 0.1.0 Scope

这个版本适合作为技术预览版使用，核心目标是验证本地收藏库闭环：

- 登录态读取和快速同步
- 收藏索引抓取、增量同步和完整同步
- 前 30 条媒体自动下载，其余按需下载
- 收藏详情、正文、封面、视频预览
- 分类、标签、状态、批注
- AI 大类归类、分类拆分、标签分组
- 本地多账号和本机安全存储

## Roadmap

- 文件 / 专辑同步：把小红书专辑和文件夹映射到本地分类或集合。
- 更可靠的详情批量补全：失败重试、限流、断点恢复和任务队列。
- 媒体下载队列：更清楚的并发控制、失败原因和重试入口。
- macOS 公证和 Windows 代码签名：降低安装时的系统安全提示。
- 自动更新：基于 GitHub Releases 或自托管更新源发布新版本。
- 更细的隐私控制：截图脱敏、导出脱敏和可选加密本地数据库。

## Install

正式构建会发布在 [GitHub Releases](https://github.com/Guesswhat-Studio/xhs-collection/releases)。0.1.0 之后的 tag 会由 CI 自动构建：

- Windows x64：NSIS / MSI
- macOS Apple Silicon：`.app` / `.dmg`
- Linux x64：AppImage / deb / rpm

macOS 未签名或未公证的构建可能会被 Gatekeeper 拦截；0.1.0 暂时按技术预览处理。

## Development

环境要求：

- Node.js LTS
- Rust stable
- Tauri v2 系统依赖

```bash
npm ci
npm run tauri:dev
```

本地构建：

```bash
npm run build
npm run tauri:build
```

常用检查：

```bash
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

## Release CI

`.github/workflows/release.yml` 会在 `main`、PR 和手动触发时构建三平台桌面包。推送 `v*` tag 时会创建 draft release 并上传产物。

```bash
git tag v0.1.0
git push origin v0.1.0
```

CI 平台矩阵：

- `windows-latest`
- `macos-latest` with `--target aarch64-apple-darwin`
- `ubuntu-22.04`

## Privacy

- 收藏、分类、标签、媒体和同步记录默认保存在本机应用数据目录。
- 小红书登录 Cookie 优先保存在系统 Keychain / Credential Store；过期后需要重新登录。
- AI API Key 保存在本机安全存储；AI 分类时会把选中的收藏标题、正文摘要和标签发送到你配置的模型服务商。
- 项目不会内置第三方 API Key。

## Tech Stack

- Tauri v2
- Rust
- React 19
- TypeScript
- SQLite
- lucide-react
