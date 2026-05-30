import { invoke } from '@tauri-apps/api/core';
import type {
  BatchNoteMetadataUpdateInput,
  AiClassifyInput,
  AiClassificationResult,
  AiSettings,
  AiSettingsInput,
  AiSettingsTestResult,
  AiSplitCategoryInput,
  AiTagGroupInput,
  AiTagGroupResult,
  BatchJobInput,
  BatchJobResult,
  ExportLibraryInput,
  ExportLibraryResult,
  LibraryApi,
  LibraryOverview,
  LocalProfileSummary,
  NoteMetadataUpdateInput,
  XhsFavoriteSyncInput,
  XhsFavoriteSyncResult,
  NoteSummary,
  StatusUpdateInput,
  TagSummary,
  XhsSessionTestResult,
} from '../types/library';

function isTauriRuntime() {
  const tauri = (window as unknown as { __TAURI_INTERNALS__?: { transformCallback?: unknown } }).__TAURI_INTERNALS__;
  return typeof tauri?.transformCallback === 'function';
}

function browserPreviewOverview(): LibraryOverview {
  const activeProfile: LocalProfileSummary = {
    id: 'local:preview',
    displayName: '本机账号',
    source: null,
    sourceAccountId: null,
    avatarUrl: null,
    dbPath: 'Tauri 桌面运行时可用',
    mediaDir: 'Tauri 桌面运行时可用',
    isActive: true,
    sessionStatus: 'unknown',
    lastOpenedAt: null,
    lastSyncAt: null,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
  return {
    appDataDir: 'Tauri 桌面运行时可用',
    dbPath: 'Tauri 桌面运行时可用',
    mediaDir: 'Tauri 桌面运行时可用',
    notesCount: 0,
    mediaCount: 0,
    storageRootId: 'default-media',
    activeProfile,
    profiles: [activeProfile],
  };
}

function desktopOnly(message: string): never {
  throw new Error(`${message} 请在 Tauri 桌面 App 中使用。`);
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => {
      window.setTimeout(() => reject(new Error(message)), timeoutMs);
    }),
  ]);
}

export const libraryApi: LibraryApi = {
  async getLibraryOverview(): Promise<LibraryOverview> {
    if (!isTauriRuntime()) {
      return browserPreviewOverview();
    }
    return invoke('get_library_overview');
  },

  async listLocalProfiles(): Promise<LocalProfileSummary[]> {
    if (!isTauriRuntime()) {
      return browserPreviewOverview().profiles;
    }
    return invoke('list_local_profiles');
  },

  async switchLocalProfile(profileId: string): Promise<LibraryOverview> {
    if (!isTauriRuntime()) {
      void profileId;
      return browserPreviewOverview();
    }
    return invoke('switch_local_profile', { profileId });
  },

  async deleteLibraryDatabase(): Promise<LibraryOverview> {
    if (!isTauriRuntime()) {
      return browserPreviewOverview();
    }
    return invoke('delete_library_database');
  },

  async clearMediaFiles(): Promise<LibraryOverview> {
    if (!isTauriRuntime()) {
      return browserPreviewOverview();
    }
    return invoke('clear_media_files');
  },

  async resetLibraryData(): Promise<LibraryOverview> {
    if (!isTauriRuntime()) {
      return browserPreviewOverview();
    }
    return invoke('reset_library_data');
  },

  async listNotes(): Promise<NoteSummary[]> {
    if (!isTauriRuntime()) {
      return [];
    }
    return invoke('list_notes');
  },

  async updateNoteStatus(input: StatusUpdateInput): Promise<NoteSummary[]> {
    if (!isTauriRuntime()) {
      void input;
      return [];
    }
    return invoke('update_note_status', { input });
  },

  async updateNoteMetadata(input: NoteMetadataUpdateInput): Promise<NoteSummary[]> {
    if (!isTauriRuntime()) {
      void input;
      return [];
    }
    return invoke('update_note_metadata', { input });
  },

  async batchUpdateNoteMetadata(input: BatchNoteMetadataUpdateInput): Promise<NoteSummary[]> {
    if (!isTauriRuntime()) {
      void input;
      return [];
    }
    return invoke('batch_update_note_metadata', { input });
  },

  async loadAiSettings(): Promise<AiSettings> {
    if (!isTauriRuntime()) {
      return {
        provider: 'openai_compatible',
        baseUrl: 'https://api.deepseek.com/v1',
        model: 'deepseek-chat',
        hasApiKey: false,
        temperature: 0.2,
        maxTokens: 4096,
        updatedAt: null,
      };
    }
    return invoke('load_ai_settings');
  },

  async saveAiSettings(input: AiSettingsInput): Promise<AiSettings> {
    if (!isTauriRuntime()) {
      return {
        provider: input.provider,
        baseUrl: input.baseUrl,
        model: input.model,
        hasApiKey: Boolean(input.apiKey),
        temperature: input.temperature ?? 0.2,
        maxTokens: input.maxTokens ?? 4096,
        updatedAt: new Date().toISOString(),
      };
    }
    return invoke('save_ai_settings', { input });
  },

  async testAiSettings(): Promise<AiSettingsTestResult> {
    if (!isTauriRuntime()) {
      desktopOnly('浏览器预览不能测试 AI API。');
    }
    return withTimeout(
      invoke('test_ai_settings'),
      180000,
      'AI 连接测试超过 3 分钟。请检查端点、模型和 API Key。',
    );
  },

  async listTags(): Promise<TagSummary[]> {
    if (!isTauriRuntime()) {
      return [];
    }
    return invoke('list_tags');
  },

  async aiClassifyUncategorized(input: AiClassifyInput): Promise<AiClassificationResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能调用 AI 分类。');
    }
    return withTimeout(
      invoke('ai_classify_uncategorized', { input }),
      1800000,
      'AI 自动分类超过 30 分钟。可以稍后缩小数量重试。',
    );
  },

  async aiSplitCategory(input: AiSplitCategoryInput): Promise<AiClassificationResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能调用 AI 分类。');
    }
    return withTimeout(
      invoke('ai_split_category', { input }),
      1800000,
      'AI 分类筛选超过 30 分钟。可以稍后缩小数量重试。',
    );
  },

  async aiGroupTags(input: AiTagGroupInput): Promise<AiTagGroupResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能调用 AI 标签整理。');
    }
    return withTimeout(
      invoke('ai_group_tags', { input }),
      1800000,
      'AI 标签整理超过 30 分钟。可以稍后缩小数量重试。',
    );
  },

  async exportLibrary(input: ExportLibraryInput): Promise<ExportLibraryResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能导出本地文件。');
    }
    return invoke('export_library', { input });
  },

  async loadXhsSavedSession(): Promise<XhsSessionTestResult | null> {
    if (!isTauriRuntime()) {
      return null;
    }
    return withTimeout(
      invoke('load_xhs_saved_session'),
      25000,
      '读取本地保存的登录态超过 25 秒。可以先打开登录窗口重新连接。',
    );
  },

  async openXhsLoginWindow(): Promise<void> {
    if (!isTauriRuntime()) {
      desktopOnly('浏览器预览不能打开小红书登录窗口。');
    }
    return invoke('open_xhs_login_window');
  },

  async readXhsLoginCookies(): Promise<XhsSessionTestResult> {
    if (!isTauriRuntime()) {
      desktopOnly('浏览器预览不能读取登录态。');
    }
    return withTimeout(
      invoke('read_xhs_login_cookies'),
      35000,
      '读取登录态超过 35 秒。请关闭登录窗口重开再试，或先使用手动 Cookie 兜底。',
    );
  },

  async testXhsSession(cookie: string): Promise<XhsSessionTestResult> {
    if (!isTauriRuntime()) {
      void cookie;
      desktopOnly('浏览器预览不能测试小红书登录态。');
    }
    return withTimeout(
      invoke('test_xhs_session', { cookie }),
      20000,
      '连接测试超过 20 秒。可能是小红书请求被网络或风控卡住了。',
    );
  },

  async syncXhsFavorites(input: XhsFavoriteSyncInput): Promise<XhsFavoriteSyncResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能同步小红书收藏。');
    }
    return withTimeout(
      invoke('sync_xhs_favorites', { input }),
      3600000,
      '同步收藏超过 60 分钟。请确认登录窗口仍打开，或稍后重试。',
    );
  },

  async enrichXhsNoteDetails(input: BatchJobInput): Promise<BatchJobResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能补全小红书笔记。');
    }
    return withTimeout(
      invoke('enrich_xhs_note_details', { input }),
      3600000,
      '补全笔记详情超过 60 分钟。请确认登录窗口仍打开，或稍后重试。',
    );
  },

  async downloadMediaAssets(input: BatchJobInput): Promise<BatchJobResult> {
    if (!isTauriRuntime()) {
      void input;
      desktopOnly('浏览器预览不能下载本地媒体。');
    }
    return withTimeout(
      invoke('download_media_assets', { input }),
      3600000,
      '媒体下载超过 60 分钟。可以稍后继续下载未完成资产。',
    );
  },
};
