import {
  Archive,
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  BookOpen,
  Bookmark,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  Clock3,
  CloudOff,
  Coffee,
  Compass,
  Cookie,
  Cpu,
  CreditCard,
  Download,
  Edit3,
  ExternalLink,
  Eye,
  EyeOff,
  FileDown,
  FileText,
  Folder,
  FolderInput,
  Gift,
  GraduationCap,
  Grid2X2,
  HardDrive,
  Heart,
  History,
  Home,
  Hourglass,
  Image as ImageIcon,
  Inbox,
  KeyRound,
  Layers,
  Library,
  List,
  Loader2,
  LogIn,
  MapPin,
  Maximize2,
  Music,
  Palette,
  Plane,
  PlayCircle,
  Plus,
  RefreshCw,
  RotateCw,
  ScanLine,
  Scissors,
  Search,
  Settings,
  ShieldCheck,
  Shirt,
  ShoppingBag,
  Smile,
  Sparkles,
  Tag,
  Tags,
  Trash2,
  User,
  Utensils,
  Video,
  Wifi,
  X,
  type LucideIcon,
} from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { useDeferredValue, useEffect, useMemo, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react';
import brandLogoUrl from '../../assets/brand/app-icon.png';
import { coveragePercent, durationFmt, fileSize, fullDate, parseAppTime, relTime, shortDate } from './formatUtils';
import { libraryApi } from './libraryApi';
import { mediaAbsolutePath, mediaAspectLabel, mediaAspectStyle, mediaPreviewSrc } from './mediaUtils';
import { coverGrad, favTimeLabel, hashSeed, matchFilter, noteFlags, noteTime, splitTagInput, type LibraryFilter } from './noteUtils';
import { isTauriRuntime, openExternalUrl, openLocalPath } from './runtime';
import type {
  AiSettings,
  AiSettingsInput,
  AiPromptEditorItem,
  AiPromptSettings,
  AiClassificationResult,
  AiJobProgress,
  BatchNoteMetadataUpdateInput,
  BatchJobProgress,
  BatchJobResult,
  DownloadStatus,
  NoteMetadataUpdateInput,
  LibraryOverview,
  LogFileInfo,
  LocalProfileSummary,
  MediaAsset,
  NoteStatus,
  NoteSummary,
  NoteType,
  TagGovernanceApplyResult,
  TagGovernanceSuggestionResult,
  TagSummary,
  XhsFavoriteSyncResult,
  XhsAlbumSyncResult,
  XhsSessionTestResult,
  XhsSyncProgress,
} from '../types/library';

type AppView = 'library' | 'tags' | 'media' | 'sync' | 'export' | 'settings';
type LibraryViewMode = 'grid' | 'list';
type Theme = 'light' | 'dark';
type FontScheme = 'sans' | 'serif' | 'kai';

interface AccountSummary {
  nickname: string;
  handle: string;
  connected: boolean;
  avatarTone: [string, string];
  avatarUrl?: string | null;
  lastSyncedAt: string;
  sessionExpiresInDays: string;
  defaultDir: string;
}

interface Tweaks {
  theme: Theme;
  font: FontScheme;
  defaultView: LibraryViewMode;
  sidebarCollapsed: boolean;
  inspectorWidth: number;
}

const TWEAK_DEFAULTS: Tweaks = {
  theme: 'light',
  font: 'sans',
  defaultView: 'grid',
  sidebarCollapsed: false,
  inspectorWidth: 560,
};

const VIEW_META: Record<AppView, { title: string; sub: string }> = {
  library: { title: '我的收藏库', sub: '搜索、复查、标记你同步到本地的小红书收藏。' },
  tags: { title: '分类与标签', sub: '用分类和标签把收藏拆成可处理的清单。' },
  media: { title: '媒体库', sub: '集中管理封面、图片、视频资产与下载状态。' },
  sync: { title: '连接与同步', sub: '连接你自己的账号，把收藏只读同步到本机。' },
  export: { title: '导出与备份', sub: '把本地收藏整理成 JSON / CSV / Markdown，或打包整库 Zip。' },
  settings: { title: '设置', sub: '外观、账号、本地存储与开发期数据。' },
};

const NAV: Array<{ view: AppView; label: string; icon: IconName }> = [
  { view: 'library', label: '收藏库', icon: 'library' },
  { view: 'tags', label: '分类标签', icon: 'tag' },
  { view: 'media', label: '媒体', icon: 'layers' },
  { view: 'sync', label: '同步', icon: 'refresh' },
  { view: 'export', label: '导出', icon: 'fileDown' },
  { view: 'settings', label: '设置', icon: 'settings' },
];

const STATUS: Record<NoteStatus, { label: string; icon: IconName }> = {
  unread: { label: '待看', icon: 'clock' },
  read: { label: '已看', icon: 'checkCircle' },
  outdated: { label: '过时', icon: 'history' },
  archived: { label: '归档', icon: 'archive' },
};

const STATUS_ORDER: NoteStatus[] = ['unread', 'read', 'outdated', 'archived'];

const NOTE_TYPE: Record<NoteType, { label: string; icon: IconName }> = {
  video: { label: '视频', icon: 'video' },
  image: { label: '图文', icon: 'image' },
  article: { label: '长文', icon: 'fileText' },
  unknown: { label: '笔记', icon: 'bookmark' },
};

const DL: Record<DownloadStatus, { label: string; cls: string; icon: IconName }> = {
  not_downloaded: { label: '未下载', cls: 'dl-not_downloaded', icon: 'download' },
  queued: { label: '排队中', cls: 'dl-queued', icon: 'hourglass' },
  downloading: { label: '下载中', cls: 'dl-downloading', icon: 'loader' },
  downloaded: { label: '已下载', cls: 'dl-downloaded', icon: 'checkCircle' },
  failed: { label: '下载失败', cls: 'dl-failed', icon: 'alert' },
};

const AI_PRESETS: Array<{
  label: string;
  provider: AiSettings['provider'];
  baseUrl: string;
  model: string;
  docsUrl?: string;
  pricingUrl?: string;
  custom?: boolean;
}> = [
  { label: 'DeepSeek', provider: 'openai_compatible', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat', docsUrl: 'https://api-docs.deepseek.com/' },
  { label: '硅基流动', provider: 'openai_compatible', baseUrl: 'https://api.siliconflow.cn/v1', model: 'Pro/zai-org/GLM-4.7', docsUrl: 'https://docs.siliconflow.cn/cn/api-reference/chat-completions/chat-completions', pricingUrl: 'https://siliconflow.cn/pricing' },
  { label: '通义千问', provider: 'openai_compatible', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-plus', docsUrl: 'https://help.aliyun.com/zh/model-studio/use-qwen-by-calling-api' },
  { label: 'Kimi', provider: 'openai_compatible', baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k', docsUrl: 'https://platform.kimi.com/docs/api/overview' },
  { label: 'MiniMax', provider: 'openai_compatible', baseUrl: 'https://api.minimax.io/v1', model: 'MiniMax-M2.7', docsUrl: 'https://platform.minimax.io/docs/token-plan/other-tools' },
  { label: '智谱 GLM', provider: 'openai_compatible', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-5.1', docsUrl: 'https://docs.bigmodel.cn/cn/guide/develop/openai/introduction' },
  { label: '腾讯混元', provider: 'openai_compatible', baseUrl: 'https://api.hunyuan.cloud.tencent.com/v1', model: 'hunyuan-turbos-latest', docsUrl: 'https://cloud.tencent.com/document/product/1729/111007' },
  { label: 'OpenAI', provider: 'openai_compatible', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4.1-mini', docsUrl: 'https://developers.openai.com/api/docs' },
  { label: 'OpenRouter', provider: 'openai_compatible', baseUrl: 'https://openrouter.ai/api/v1', model: 'openai/gpt-4.1-mini', docsUrl: 'https://openrouter.ai/docs/api/reference/overview' },
  { label: 'Claude', provider: 'claude', baseUrl: 'https://api.anthropic.com/v1', model: 'claude-sonnet-4-5', docsUrl: 'https://platform.claude.com/docs/en/api/overview' },
  { label: '自定义', provider: 'openai_compatible', baseUrl: '', model: '', custom: true },
];

const MEDIA_TYPE_LABEL: Record<MediaAsset['mediaType'], string> = {
  video: '视频',
  image: '图片',
  cover: '封面',
  file: '文件',
};

function hostOf(url?: string | null) {
  try {
    return new URL(url || '').host;
  } catch {
    return (url || '').replace(/^https?:\/\//, '').split('/')[0] || '—';
  }
}

function aiSettingsReady(settings: AiSettings | null) {
  return Boolean(settings?.hasApiKey && settings.baseUrl && settings.model);
}

const ICONS = {
  alert: AlertTriangle,
  archive: Archive,
  arrowLeft: ArrowLeft,
  arrowRight: ArrowRight,
  book: BookOpen,
  bookmark: Bookmark,
  briefcase: Gift,
  camera: ImageIcon,
  check: Check,
  checkCircle: CheckCircle2,
  chevronDown: ChevronDown,
  chevronRight: ChevronRight,
  clock: Clock3,
  circleHelp: CircleHelp,
  cloudOff: CloudOff,
  coffee: Coffee,
  compass: Compass,
  cookie: Cookie,
  cpu: Cpu,
  creditCard: CreditCard,
  download: Download,
  edit: Edit3,
  externalLink: ExternalLink,
  eye: Eye,
  eyeOff: EyeOff,
  fileDown: FileDown,
  fileText: FileText,
  folder: Folder,
  folderInput: FolderInput,
  gift: Gift,
  graduationCap: GraduationCap,
  grid: Grid2X2,
  hardDrive: HardDrive,
  heart: Heart,
  history: History,
  home: Home,
  hourglass: Hourglass,
  image: ImageIcon,
  inbox: Inbox,
  key: KeyRound,
  layers: Layers,
  library: Library,
  list: List,
  loader: Loader2,
  login: LogIn,
  mapPin: MapPin,
  maximize: Maximize2,
  music: Music,
  palette: Palette,
  plane: Plane,
  playLg: PlayCircle,
  plus: Plus,
  refresh: RefreshCw,
  rotateCw: RotateCw,
  scan: ScanLine,
  scissors: Scissors,
  search: Search,
  searchX: Search,
  settings: Settings,
  shieldCheck: ShieldCheck,
  shirt: Shirt,
  shoppingBag: ShoppingBag,
  smile: Smile,
  sparkles: Sparkles,
  tag: Tag,
  tags: Tags,
  trash: Trash2,
  user: User,
  utensils: Utensils,
  video: Video,
  wifi: Wifi,
  x: X,
} satisfies Record<string, LucideIcon>;

type IconName = keyof typeof ICONS;

function Icon({ name, size = 16, stroke = 2, className, style }: {
  name: IconName;
  size?: number;
  stroke?: number;
  className?: string;
  style?: CSSProperties | null;
}) {
  const Cmp = ICONS[name] ?? Bookmark;
  return <Cmp aria-hidden="true" className={className} size={size} strokeWidth={stroke} style={style ?? undefined} />;
}

function useTweaks() {
  const [t, setTweaks] = useState<Tweaks>(() => {
    try {
      const raw = localStorage.getItem('xhs_design_tweaks');
      const parsed = raw ? JSON.parse(raw) as Partial<Tweaks> : {};
      return {
        theme: parsed.theme ?? TWEAK_DEFAULTS.theme,
        font: parsed.font ?? TWEAK_DEFAULTS.font,
        defaultView: parsed.defaultView ?? TWEAK_DEFAULTS.defaultView,
        sidebarCollapsed: parsed.sidebarCollapsed ?? TWEAK_DEFAULTS.sidebarCollapsed,
        inspectorWidth: parsed.inspectorWidth ?? TWEAK_DEFAULTS.inspectorWidth,
      };
    } catch {
      return TWEAK_DEFAULTS;
    }
  });

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute('data-theme', t.theme);
    root.setAttribute('data-font', t.font);
    try {
      localStorage.setItem('xhs_design_tweaks', JSON.stringify(t));
    } catch {
      // Ignore localStorage failures in locked-down preview contexts.
    }
  }, [t]);

  function setTweak<K extends keyof Tweaks>(key: K, value: Tweaks[K]) {
    setTweaks((current) => ({ ...current, [key]: value }));
  }

  return [t, setTweak] as const;
}

export function App() {
  const [t, setTweak] = useTweaks();
  const [overview, setOverview] = useState<LibraryOverview | null>(null);
  const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [tagSummaries, setTagSummaries] = useState<TagSummary[]>([]);
  const [aiSettings, setAiSettings] = useState<AiSettings | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<LibraryFilter>('all');
  const [query, setQuery] = useState('');
  const [categoryScope, setCategoryScope] = useState<string | null>(null);
  const [tagScope, setTagScope] = useState<string | null>(null);
  const [view, setView] = useState<AppView>('library');
  const [viewMode, setViewMode] = useState<LibraryViewMode>(t.defaultView);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [isLoading, setIsLoading] = useState(true);
  const [isResetting, setIsResetting] = useState(false);
  const [isDeletingDatabase, setIsDeletingDatabase] = useState(false);
  const [isClearingMedia, setIsClearingMedia] = useState(false);
  const [message, setMessage] = useState('正在读取本地收藏库...');
  const [xhsSession, setXhsSession] = useState<XhsSessionTestResult | null>(null);
  const [profileGateOpen, setProfileGateOpen] = useState(false);
  const [profileGateTouched, setProfileGateTouched] = useState(false);

  async function loadLibrary(nextMessage?: string) {
    setIsLoading(true);
    try {
      const [overviewResult, noteResult] = await Promise.all([
        libraryApi.getLibraryOverview(),
        libraryApi.listNotes(),
      ]);
      const [tagResult, aiResult] = await Promise.all([
        libraryApi.listTags(),
        libraryApi.loadAiSettings(),
      ]);
      setOverview(overviewResult);
      setNotes(noteResult);
      setTagSummaries(tagResult);
      setAiSettings(aiResult);
      setSelectedId((current) => (current && noteResult.some((note) => note.id === current) ? current : noteResult[0]?.id ?? null));
      setMessage(nextMessage ?? '本地库已就绪');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '本地库初始化失败');
    } finally {
      setIsLoading(false);
    }
  }

  async function refreshSavedSession() {
    try {
      const session = await libraryApi.loadXhsSavedSession();
      setXhsSession(session);
      if (session?.ok) {
        const overviewResult = await libraryApi.getLibraryOverview();
        setOverview(overviewResult);
      }
      return session;
    } catch {
      setXhsSession(null);
      return null;
    }
  }

  async function handleSwitchLocalProfile(profileId: string) {
    setIsLoading(true);
    setMessage('正在切换本地账号...');
    try {
      const overviewResult = await libraryApi.switchLocalProfile(profileId);
      const [noteResult, tagResult, aiResult] = await Promise.all([
        libraryApi.listNotes(),
        libraryApi.listTags(),
        libraryApi.loadAiSettings(),
      ]);
      setOverview(overviewResult);
      setNotes(noteResult);
      setTagSummaries(tagResult);
      setAiSettings(aiResult);
      setSelectedId((current) => (current && noteResult.some((note) => note.id === current) ? current : noteResult[0]?.id ?? null));
      await refreshSavedSession();
      setMessage(`已切换到「${overviewResult.activeProfile.displayName}」`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '切换本地账号失败');
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    loadLibrary();
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return undefined;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    listen<string>('library-changed', (event) => {
      if (!disposed) void loadLibrary(event.payload);
    })
      .then((unlisten) => {
        cleanup = unlisten;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    void refreshSavedSession();
  }, []);

  useEffect(() => {
    if ((overview?.profiles.length ?? 0) > 1 && !profileGateTouched) {
      setProfileGateOpen(true);
    }
  }, [overview?.profiles.length, profileGateTouched]);

  const counts = useMemo(() => {
    return notes.reduce(
      (acc, note) => {
        acc[note.status] += 1;
        const flags = noteFlags(note);
        if (flags.remoteMissing || flags.failed || flags.coverMissing) acc.attention += 1;
        return acc;
      },
      { unread: 0, read: 0, outdated: 0, archived: 0, attention: 0, total: notes.length },
    );
  }, [notes]);

  const account: AccountSummary = useMemo(() => {
    const activeProfile = overview?.activeProfile;
    const connected = Boolean(xhsSession?.ok) || activeProfile?.sessionStatus === 'connected';
    const accountName = xhsSession?.accountName?.trim() || activeProfile?.displayName?.trim();
    const accountId = xhsSession?.accountId?.trim() || activeProfile?.sourceAccountId?.trim();
    return {
      nickname: accountName || (connected ? '小红书账号' : '本机账号'),
      handle: accountId ? `xhs:${accountId}` : 'xiaohongshu.com',
      connected,
      avatarTone: connected ? ['#f29bb6', '#e2588a'] : ['#c8bcc0', '#897e84'],
      avatarUrl: xhsSession?.avatarUrl ?? activeProfile?.avatarUrl ?? null,
      lastSyncedAt: activeProfile?.lastSyncAt ?? notes[0]?.lastSyncedAt ?? xhsSession?.checkedAt ?? new Date().toISOString(),
      sessionExpiresInDays: connected ? 'Keychain / 本地已保存' : '未知',
      defaultDir: overview?.appDataDir ?? '初始化中...',
    };
  }, [notes, overview, xhsSession]);

  const meta = VIEW_META[view];
  const libraryMessage = isLoading
    ? '正在读取本地收藏库...'
    : `${notes.length} 条收藏 · ${counts.unread} 条待看${counts.attention ? ` · ${counts.attention} 条需处理` : ''}`;

  async function handleStatusChange(noteId: string, status: NoteStatus) {
    try {
      const updated = await libraryApi.updateNoteStatus({ noteId, status });
      setNotes(updated);
      setMessage(`状态已更新为「${STATUS[status].label}」`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '状态更新失败');
    }
  }

  async function handleSaveUserNote(noteId: string, userNote: string) {
    await handleUpdateNoteMetadata({ noteId, userNote });
  }

  async function handleUpdateNoteMetadata(input: NoteMetadataUpdateInput) {
    try {
      const updated = await libraryApi.updateNoteMetadata(input);
      setNotes(updated);
      setMessage('收藏整理信息已保存到 SQLite');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '保存收藏整理信息失败');
      throw error;
    }
  }

  async function handleBatchUpdateNotes(input: BatchNoteMetadataUpdateInput) {
    try {
      const updated = await libraryApi.batchUpdateNoteMetadata(input);
      setNotes(updated);
      setMessage(`已批量更新 ${input.noteIds.length} 条收藏`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '批量更新失败');
      throw error;
    }
  }

  async function handleDownloadNoteMedia(noteId: string) {
    try {
      const result = await libraryApi.downloadMediaAssets({ noteId, limit: 100 });
      await loadLibrary(result.message);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '下载当前笔记媒体失败');
      throw error;
    }
  }

  async function handleResetLibraryData() {
    if (!window.confirm('清空本地 SQLite、同步记录和媒体目录？该操作不会影响小红书账号。')) {
      return;
    }
    setIsResetting(true);
    setMessage('正在清空本地库');
    try {
      const overviewResult = await libraryApi.resetLibraryData();
      setOverview(overviewResult);
      setNotes([]);
      setTagSummaries([]);
      setSelectedId(null);
      setView('library');
      setMessage('本地库已清空');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '清空本地库失败');
    } finally {
      setIsResetting(false);
    }
  }

  async function handleDeleteLibraryDatabase() {
    if (!window.confirm('删除当前本地账号的 SQLite 数据库？媒体文件会保留，但收藏、标签和同步记录会清空。')) {
      return;
    }
    setIsDeletingDatabase(true);
    setMessage('正在删除 SQLite 数据库');
    try {
      const overviewResult = await libraryApi.deleteLibraryDatabase();
      setOverview(overviewResult);
      setNotes([]);
      setTagSummaries([]);
      setSelectedId(null);
      setView('library');
      setMessage('SQLite 数据库已删除并重新初始化');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '删除数据库失败');
    } finally {
      setIsDeletingDatabase(false);
    }
  }

  async function handleClearMediaFiles() {
    if (!window.confirm('清空当前本地账号的媒体目录？收藏记录会保留，媒体下载状态会重置为未下载。')) {
      return;
    }
    setIsClearingMedia(true);
    setMessage('正在清空媒体文件');
    try {
      const overviewResult = await libraryApi.clearMediaFiles();
      const noteResult = await libraryApi.listNotes();
      setOverview(overviewResult);
      setNotes(noteResult);
      setSelectedId((current) => (current && noteResult.some((note) => note.id === current) ? current : noteResult[0]?.id ?? null));
      setMessage('媒体文件已清空，下载状态已重置');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : '清空媒体文件失败');
    } finally {
      setIsClearingMedia(false);
    }
  }

  function gotoLibrary({
    q = '',
    f = 'all',
    id = null,
    category = null,
    tag = null,
  }: {
    q?: string;
    f?: LibraryFilter;
    id?: string | null;
    category?: string | null;
    tag?: string | null;
  } = {}) {
    setView('library');
    setQuery(q);
    setFilter(f);
    setCategoryScope(category);
    setTagScope(tag);
    if (id) {
      setSelectedId(id);
      setInspectorOpen(true);
    }
  }

  const inspectorWidth = Math.max(420, Math.min(860, t.inspectorWidth || TWEAK_DEFAULTS.inspectorWidth));

  return (
    <div className="win">
      <div className={`body ${t.sidebarCollapsed ? 'sidebar-collapsed' : ''}`}>
        <aside className="sidebar">
          <div className="brand">
            <div className="brand-lockup">
              <div className="brand-mark">
                <img alt="" src={brandLogoUrl} />
              </div>
              <div className="brand-text">
                <strong>XHS Collection</strong>
                <span>本地收藏复查台</span>
              </div>
            </div>
            <button
              className="side-toggle"
              onClick={() => setTweak('sidebarCollapsed', !t.sidebarCollapsed)}
              title={t.sidebarCollapsed ? '展开侧栏' : '收起侧栏'}
              type="button"
            >
              <Icon name={t.sidebarCollapsed ? 'arrowRight' : 'arrowLeft'} size={15} />
            </button>
          </div>

          <nav className="nav" aria-label="主导航">
            <div className="nav-label">工作台</div>
            {NAV.slice(0, 3).map((item) => (
              <button
                className={`nav-item ${view === item.view ? 'active' : ''}`}
                key={item.view}
                onClick={() => setView(item.view)}
                type="button"
              >
                <Icon name={item.icon} size={18} />
                <span>{item.label}</span>
                {item.view === 'library' && counts.attention > 0 && (
                  <span className="nav-dot" title={`${counts.attention} 条需处理`} />
                )}
              </button>
            ))}

            <div className="nav-label nav-label-data">数据</div>
            {NAV.slice(3).map((item) => (
              <button
                className={`nav-item ${view === item.view ? 'active' : ''}`}
                key={item.view}
                onClick={() => setView(item.view)}
                type="button"
              >
                <Icon name={item.icon} size={18} />
                <span>{item.label}</span>
              </button>
            ))}
          </nav>

          <div className="side-spacer" />

          <div className="side-card account-card">
            <div className="acct">
              <span className="acct-avatar" style={{ background: `linear-gradient(140deg, ${account.avatarTone[0]}, ${account.avatarTone[1]})` }}>
                {account.avatarUrl ? <img alt="" src={account.avatarUrl} /> : account.nickname.slice(0, 1)}
              </span>
              <div className="acct-info">
                <strong>{account.nickname}</strong>
                <span>
                  <span className="acct-live" />
                  {account.connected ? `${relTime(account.lastSyncedAt)}同步` : '未连接小红书'}
                </span>
              </div>
            </div>
          </div>

          <div className="side-card storage">
            <Icon name="hardDrive" size={16} />
            <div>
              <strong>本地存储</strong>
              <p>{overview?.appDataDir ?? '初始化中...'}</p>
            </div>
          </div>
        </aside>

        <main className="workspace">
          <header className="topbar">
            <div className="topbar-head">
              <h1>{meta.title}</h1>
              <p>
                {view === 'library' ? (
                  <>
                    <Icon name="bookmark" size={13} />
                    {libraryMessage}
                  </>
                ) : (
                  meta.sub
                )}
              </p>
            </div>

            <div className="topbar-actions">
              {view === 'library' && (
                <>
                  <div className="search">
                    <Icon name="search" size={17} />
                    <input
                      aria-label="搜索收藏"
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder="搜索标题、作者、标签、正文..."
                      value={query}
                    />
                    {query && (
                      <button className="btn-quiet clear-search" onClick={() => setQuery('')} type="button" title="清除搜索">
                        <Icon name="x" size={15} />
                      </button>
                    )}
                  </div>
                  <button className="btn btn-ghost" disabled={isLoading} onClick={() => loadLibrary()} type="button">
                    <Icon name="refresh" size={16} />
                    刷新
                  </button>
                  <button className="btn btn-primary" onClick={() => setView('sync')} type="button">
                    <Icon name="refresh" size={16} />
                    同步收藏
                  </button>
                </>
              )}
              {view === 'tags' && (
                <button className="btn btn-ghost" onClick={() => setView('library')} type="button">
                  <Icon name="library" size={16} />
                  返回收藏库
                </button>
              )}
              {view === 'media' && (
                <button className="btn btn-ghost" onClick={() => setView('sync')} type="button">
                  <Icon name="download" size={16} />
                  同步以发现更多
                </button>
              )}
              {view === 'export' && (
                <button className="btn btn-ghost" onClick={() => setView('settings')} type="button">
                  <Icon name="hardDrive" size={16} />
                  存储设置
                </button>
              )}
            </div>
          </header>

          {view === 'library' ? (
            <div className="view-scroll view-scroll-library">
              {isLoading ? (
                <>
                  <div className="quickbar">
                    {Array.from({ length: 4 }).map((_, index) => (
                      <div className="sk" key={index} style={{ height: 58, width: 150, borderRadius: 14 }} />
                    ))}
                  </div>
                  <SkeletonGrid />
                </>
              ) : notes.length === 0 ? (
                <div className="panel empty-panel">
                  <EmptyState
                    actions={
                      <button className="btn btn-primary" onClick={() => setView('sync')} type="button">
                        <Icon name="refresh" size={16} />
                        前往同步
                      </button>
                    }
                    desc="还没有任何收藏。连接你的小红书账号，把收藏只读同步到本机，就能在这里搜索、复查和整理。"
                    icon="bookmark"
                    title="本地收藏库是空的"
                  />
                </div>
              ) : (
                <LibraryView
                  filter={filter}
                  categoryScope={categoryScope}
                  inspectorOpen={inspectorOpen}
                  notes={notes}
                  onClearQuery={() => setQuery('')}
                  onClearScope={() => {
                    setCategoryScope(null);
                    setTagScope(null);
                  }}
                  onBatchUpdate={handleBatchUpdateNotes}
                  onDownloadNoteMedia={handleDownloadNoteMedia}
                  inspectorWidth={inspectorWidth}
                  onSaveNote={handleSaveUserNote}
                  onStatusChange={handleStatusChange}
                  onTagClick={(tag) => {
                    setQuery('');
                    setCategoryScope(null);
                    setTagScope(tag);
                    setFilter('all');
                  }}
                  onInspectorWidthChange={(width) => setTweak('inspectorWidth', width)}
                  onUpdateNote={handleUpdateNoteMetadata}
                  overview={overview}
                  query={query}
                  selectedId={selectedId}
                  setFilter={setFilter}
                  setInspectorOpen={setInspectorOpen}
                  setSelectedId={setSelectedId}
                  tagScope={tagScope}
                  setViewMode={(next) => {
                    setViewMode(next);
                    setTweak('defaultView', next);
                  }}
                  viewMode={viewMode}
                />
              )}
            </div>
          ) : (
            <div className="view-scroll">
              {view === 'sync' && (
                <SyncView
                  overview={overview}
                  savedSession={xhsSession}
                  onSwitchProfile={handleSwitchLocalProfile}
                  onSessionChange={(session) => {
                    setXhsSession(session);
                    void loadLibrary(session?.ok ? '账号已连接，本地账号已校准' : undefined);
                  }}
                  onSynced={(summary) => loadLibrary(summary.message)}
                />
              )}
              {view === 'media' && (
                <MediaView
                  notes={notes}
                  onChanged={(nextMessage) => loadLibrary(nextMessage)}
                  onOpenNote={(id) => gotoLibrary({ id })}
                  overview={overview}
                />
              )}
              {view === 'tags' && (
                <TagsView
                  aiSettings={aiSettings}
                  notes={notes}
                  onChanged={(nextMessage) => loadLibrary(nextMessage)}
                  onCategoryClick={(category) => gotoLibrary({ category })}
                  onOpenSettings={() => setView('settings')}
                  onTagClick={(tag) => gotoLibrary({ tag })}
                  overview={overview}
                  tags={tagSummaries}
                />
              )}
              {view === 'export' && <ExportView notes={notes} overview={overview} />}
              {view === 'settings' && (
                <SettingsView
                  account={account}
                  aiSettings={aiSettings}
                  isClearingMedia={isClearingMedia}
                  isDeletingDatabase={isDeletingDatabase}
                  isResetting={isResetting}
                  notesCount={notes.length}
                  onClearMedia={handleClearMediaFiles}
                  onDeleteDatabase={handleDeleteLibraryDatabase}
                  onGoSync={() => setView('sync')}
                  onReset={handleResetLibraryData}
                  onAiSettingsChange={setAiSettings}
                  onSwitchProfile={handleSwitchLocalProfile}
                  overview={overview}
                  setTweak={setTweak}
                  t={t}
                />
              )}
            </div>
          )}
          {message && <div className="sr-only" aria-live="polite">{message}</div>}
        </main>
      </div>
      {profileGateOpen && overview && (
        <ProfileGate
          overview={overview}
          onClose={() => {
            setProfileGateTouched(true);
            setProfileGateOpen(false);
          }}
          onConnectNew={() => {
            setProfileGateTouched(true);
            setProfileGateOpen(false);
            setView('sync');
          }}
          onSelect={async (profileId) => {
            setProfileGateTouched(true);
            setProfileGateOpen(false);
            await handleSwitchLocalProfile(profileId);
          }}
        />
      )}
    </div>
  );
}

function SkeletonGrid() {
  return (
    <div className="note-grid">
      {Array.from({ length: 8 }).map((_, index) => (
        <div className="card" key={index}>
          <div className="sk card-skeleton-cover" />
          <div className="card-body">
            <div className="sk" style={{ height: 14, width: '90%' }} />
            <div className="sk" style={{ height: 12, width: '55%' }} />
            <div className="sk" style={{ height: 20, width: '40%', marginTop: 4 }} />
          </div>
        </div>
      ))}
    </div>
  );
}

function LibraryView({
  notes,
  overview,
  query,
  categoryScope,
  tagScope,
  onClearQuery,
  onClearScope,
  filter,
  setFilter,
  viewMode,
  setViewMode,
  selectedId,
  setSelectedId,
  onStatusChange,
  onTagClick,
  onSaveNote,
  onUpdateNote,
  onBatchUpdate,
  onDownloadNoteMedia,
  inspectorWidth,
  onInspectorWidthChange,
  inspectorOpen,
  setInspectorOpen,
}: {
  notes: NoteSummary[];
  overview: LibraryOverview | null;
  query: string;
  categoryScope: string | null;
  tagScope: string | null;
  onClearQuery: () => void;
  onClearScope: () => void;
  filter: LibraryFilter;
  setFilter: (filter: LibraryFilter) => void;
  viewMode: LibraryViewMode;
  setViewMode: (mode: LibraryViewMode) => void;
  selectedId: string | null;
  setSelectedId: (id: string | null) => void;
  onStatusChange: (noteId: string, status: NoteStatus) => void;
  onTagClick: (tag: string) => void;
  onSaveNote: (noteId: string, userNote: string) => Promise<void> | void;
  onUpdateNote: (input: NoteMetadataUpdateInput) => Promise<void> | void;
  onBatchUpdate: (input: BatchNoteMetadataUpdateInput) => Promise<void> | void;
  onDownloadNoteMedia: (noteId: string) => Promise<void> | void;
  inspectorWidth: number;
  onInspectorWidthChange: (width: number) => void;
  inspectorOpen: boolean;
  setInspectorOpen: (open: boolean) => void;
}) {
  const [sort, setSort] = useState<'recent' | 'oldest' | 'published'>('recent');
  const [bulkStatus, setBulkStatus] = useState<NoteStatus | ''>('');
  const [bulkCategory, setBulkCategory] = useState('');
  const [bulkTags, setBulkTags] = useState('');
  const [isBulkSaving, setIsBulkSaving] = useState(false);
  const deferredQuery = useDeferredValue(query);

  const counts = useMemo(() => {
    const result = { all: notes.length, unread: 0, read: 0, outdated: 0, archived: 0, attention: 0 };
    for (const note of notes) {
      result[note.status] += 1;
      const flags = noteFlags(note);
      if (flags.remoteMissing || flags.failed || flags.coverMissing) result.attention += 1;
    }
    return result;
  }, [notes]);

  const filtered = useMemo(() => {
    const normalizedQuery = deferredQuery.trim().toLowerCase();
    const normalizedCategoryScope = categoryScope?.trim().toLowerCase() ?? '';
    const normalizedTagScope = tagScope?.trim().toLowerCase() ?? '';
    return notes
      .filter((note) => {
        if (!matchFilter(note, filter)) return false;
        if (normalizedCategoryScope && (note.categoryName || '未分类').trim().toLowerCase() !== normalizedCategoryScope) {
          return false;
        }
        if (normalizedTagScope && !note.tags.some((tag) => tag.trim().toLowerCase() === normalizedTagScope)) {
          return false;
        }
        if (!normalizedQuery) return true;
        const searchable = [note.title, note.excerpt, note.content, note.authorName, note.categoryName, note.tags.join(' ')]
          .filter(Boolean)
          .join(' ')
          .toLowerCase();
        return searchable.includes(normalizedQuery);
      })
      .sort((a, b) => {
        if (sort === 'oldest') return noteTime(a) - noteTime(b);
        if (sort === 'published') return parseAppTime(b.publishedAt) - parseAppTime(a.publishedAt);
        return noteTime(b) - noteTime(a);
      });
  }, [categoryScope, deferredQuery, filter, notes, sort, tagScope]);

  const progressiveFiltered = useProgressiveItems(
    filtered,
    viewMode === 'grid' ? 72 : 120,
    viewMode === 'grid' ? 72 : 160,
  );

  const selected = filtered.find((note) => note.id === selectedId) ?? filtered[0] ?? null;
  const open = inspectorOpen && Boolean(selected);
  const activeScopeLabel = categoryScope ? `分类：${categoryScope}` : tagScope ? `标签：${tagScope}` : '';
  const categoryOptions = useMemo(() => {
    const names = new Set<string>();
    for (const note of notes) {
      const name = note.categoryName?.trim();
      if (name && name !== '未分类') names.add(name);
    }
    return [...names].sort((a, b) => a.localeCompare(b, 'zh-Hans-CN'));
  }, [notes]);

  function pickNote(id: string) {
    setSelectedId(id);
    setInspectorOpen(true);
  }

  function startInspectorResize(event: ReactPointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = inspectorWidth;
    const maxWidth = Math.max(420, Math.min(900, window.innerWidth - 460));

    function handleMove(moveEvent: PointerEvent) {
      const nextWidth = Math.max(420, Math.min(maxWidth, startWidth - (moveEvent.clientX - startX)));
      onInspectorWidthChange(nextWidth);
    }

    function handleUp() {
      document.removeEventListener('pointermove', handleMove);
      document.removeEventListener('pointerup', handleUp);
      document.body.classList.remove('is-resizing-inspector');
    }

    document.body.classList.add('is-resizing-inspector');
    document.addEventListener('pointermove', handleMove);
    document.addEventListener('pointerup', handleUp, { once: true });
  }

  async function applyBulkUpdate() {
    const addTags = splitTagInput(bulkTags);
    if (!filtered.length || (!bulkStatus && !bulkCategory.trim() && addTags.length === 0)) return;
    setIsBulkSaving(true);
    try {
      await onBatchUpdate({
        noteIds: filtered.map((note) => note.id),
        status: bulkStatus || undefined,
        categoryName: bulkCategory.trim() || undefined,
        addTags,
      });
      setBulkStatus('');
      setBulkCategory('');
      setBulkTags('');
    } finally {
      setIsBulkSaving(false);
    }
  }

  const quickFilters: Array<{ key: LibraryFilter; label: string; icon: IconName; tone?: string; cls?: string }> = [
    { key: 'all', label: '全部收藏', icon: 'library', tone: 'tone-all' },
    { key: 'unread', label: '待看', icon: 'history', tone: 'tone-unread' },
    { key: 'outdated', label: '过时待复查', icon: 'history', cls: 'alert' },
    { key: 'attention', label: '需要处理', icon: 'alert', cls: 'danger' },
  ];

  return (
    <div className={`lib ${open ? '' : 'collapsed'}`} style={{ '--insp-w': `${inspectorWidth}px` } as CSSProperties}>
      <div className="lib-main">
        {activeScopeLabel && (
          <div className="scopebar">
            <span>
              <Icon name={categoryScope ? 'folder' : 'tag'} size={14} />
              {activeScopeLabel}
            </span>
            <button className="btn btn-quiet btn-sm" onClick={onClearScope} type="button">
              <Icon name="x" size={14} />
              查看全部
            </button>
          </div>
        )}

        <div className="quickbar">
          {quickFilters.map((item) => (
            <button
              className={`qstat ${item.cls ?? ''} ${filter === item.key ? 'active' : ''}`}
              key={item.key}
              onClick={() => setFilter(item.key)}
              type="button"
            >
              <span className={`qicon ${item.tone ?? ''}`}>
                <Icon name={item.icon} size={18} />
              </span>
              <span>
                <span className="qv">{counts[item.key]}</span>
                <span className="ql">{item.label}</span>
              </span>
            </button>
          ))}
        </div>

        <div className="lib-toolbar">
          <div className="seg">
            {(['all', 'unread', 'read', 'outdated', 'archived'] as Array<NoteStatus | 'all'>).map((key) => (
              <button className={filter === key ? 'active' : ''} key={key} onClick={() => setFilter(key)} type="button">
                {key === 'all' ? '全部' : STATUS[key].label}
                <span className="seg-count">{counts[key]}</span>
              </button>
            ))}
          </div>
          <div className="spacer" />
          <span className="count">{filtered.length} 条</span>
          <select className="sort-select" onChange={(event) => setSort(event.target.value as typeof sort)} value={sort}>
            <option value="recent">最近收藏</option>
            <option value="oldest">最早收藏</option>
            <option value="published">按发布时间</option>
          </select>
          <div className="seg icons">
            <button className={viewMode === 'grid' ? 'active' : ''} onClick={() => setViewMode('grid')} title="网格" type="button">
              <Icon name="grid" size={16} />
            </button>
            <button className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')} title="列表" type="button">
              <Icon name="list" size={16} />
            </button>
          </div>
        </div>

        <div className="bulkbar">
          <span className="bulk-label">{filtered.length} 条当前筛选</span>
          <select className="sort-select" onChange={(event) => setBulkStatus(event.target.value as NoteStatus | '')} value={bulkStatus}>
            <option value="">状态不变</option>
            {STATUS_ORDER.map((status) => (
              <option key={status} value={status}>{STATUS[status].label}</option>
            ))}
          </select>
          <CategoryInput
            className="bulk-category-field"
            compact
            onChange={setBulkCategory}
            options={categoryOptions}
            placeholder="批量分类"
            value={bulkCategory}
          />
          <input
            className="meta-input compact-input"
            onChange={(event) => setBulkTags(event.target.value)}
            placeholder="追加标签，用逗号分隔"
            value={bulkTags}
          />
          <button
            className="btn btn-ghost btn-sm"
            disabled={isBulkSaving || filtered.length === 0 || (!bulkStatus && !bulkCategory.trim() && !bulkTags.trim())}
            onClick={() => void applyBulkUpdate()}
            type="button"
          >
            <Icon name={isBulkSaving ? 'loader' : 'check'} size={15} className={isBulkSaving ? 'spin' : ''} />
            应用
          </button>
        </div>

        <div className="lib-scroll">
          {filtered.length === 0 ? (
            query.trim() ? (
              <EmptyState
                actions={
                  <button className="btn btn-ghost btn-sm" onClick={onClearQuery} type="button">
                    <Icon name="x" size={15} />
                    清除搜索
                  </button>
                }
                desc={`没有找到包含「${query.trim()}」的笔记。试试更短的关键词，或清除搜索查看全部。`}
                icon="searchX"
                title="没有匹配的收藏"
              />
            ) : (
              <EmptyState
                actions={activeScopeLabel ? (
                  <button className="btn btn-ghost btn-sm" onClick={onClearScope} type="button">
                    <Icon name="x" size={15} />
                    查看全部
                  </button>
                ) : undefined}
                desc={activeScopeLabel ? `${activeScopeLabel} 里没有符合当前状态的收藏。` : '切换到其他状态，或前往「同步」把小红书收藏导入本地。'}
                icon="inbox"
                title="这里还没有收藏"
              />
            )
          ) : viewMode === 'grid' ? (
            <div className="note-grid fade-in">
              {progressiveFiltered.items.map((note) => (
                <NoteCard
                  isSelected={selected?.id === note.id && open}
                  key={note.id}
                  note={note}
                  onClick={() => pickNote(note.id)}
                  overview={overview}
                />
              ))}
              <ProgressiveListTail shown={progressiveFiltered.visibleCount} total={filtered.length} />
            </div>
          ) : (
            <div className="note-list fade-in">
              {progressiveFiltered.items.map((note) => (
                <NoteRow
                  isSelected={selected?.id === note.id && open}
                  key={note.id}
                  note={note}
                  onClick={() => pickNote(note.id)}
                  overview={overview}
                />
              ))}
              <ProgressiveListTail shown={progressiveFiltered.visibleCount} total={filtered.length} />
            </div>
          )}
        </div>
      </div>

      <button
        aria-label="调整详情栏宽度"
        className="insp-resizer"
        disabled={!open}
        onDoubleClick={() => onInspectorWidthChange(TWEAK_DEFAULTS.inspectorWidth)}
        onPointerDown={startInspectorResize}
        title="拖拽调整详情栏宽度，双击恢复默认"
        type="button"
      />

      <Inspector
        note={open ? selected : null}
        categoryOptions={categoryOptions}
        onClose={() => setInspectorOpen(false)}
        onDownloadNoteMedia={onDownloadNoteMedia}
        onSaveNote={onSaveNote}
        onStatusChange={onStatusChange}
        onTagClick={onTagClick}
        onUpdateNote={onUpdateNote}
        overview={overview}
      />
    </div>
  );
}

function NoteCard({
  note,
  isSelected,
  onClick,
  overview,
}: {
  note: NoteSummary;
  isSelected: boolean;
  onClick: () => void;
  overview: LibraryOverview | null;
}) {
  const flags = noteFlags(note);
  const imgCount = note.media.filter((asset) => asset.mediaType === 'image' || asset.mediaType === 'cover').length;
  return (
    <button className={`card ${isSelected ? 'selected' : ''}`} onClick={onClick} type="button">
      <Cover
        durationMs={note.noteType === 'video' ? note.media.find((asset) => asset.mediaType === 'video')?.durationMs : null}
        glyphSize={66}
        note={note}
        overview={overview}
        showMissing
        showPlay
        showType
      />
      <div className="card-body">
        <div className="card-title">{note.title}</div>
        <div className="card-author">
          <Avatar name={note.authorName} size={18} />
          <span>{note.authorName}</span>
        </div>
        <div className="card-foot">
          <StatusPill status={note.status} />
          <div className="spacer" />
          {flags.remoteMissing && (
            <span className="pill missing" title="远端已删除">
              <Icon name="cloudOff" size={12} stroke={2.3} />
            </span>
          )}
          {imgCount > 1 && (
            <span className="card-meta">
              <Icon name="image" size={13} />
              {imgCount}
            </span>
          )}
        </div>
      </div>
    </button>
  );
}

function NoteRow({
  note,
  isSelected,
  onClick,
  overview,
}: {
  note: NoteSummary;
  isSelected: boolean;
  onClick: () => void;
  overview: LibraryOverview | null;
}) {
  const flags = noteFlags(note);
  return (
    <button className={`row ${isSelected ? 'selected' : ''}`} onClick={onClick} type="button">
      <Cover glyphSize={30} note={note} overview={overview} showPlay showType={false} />
      <div className="row-main">
        <div className="row-title">
          <strong>{note.title}</strong>
          {flags.remoteMissing && (
            <span className="pill missing">
              <Icon name="cloudOff" size={11} stroke={2.3} />
              远端缺失
            </span>
          )}
        </div>
        <p className="row-excerpt">{note.excerpt}</p>
        <div className="row-meta">
          <span>{note.categoryName || '未分类'}</span>
          <span className="dot" />
          <Avatar name={note.authorName} size={14} />
          <span>{note.authorName}</span>
          <span className="dot" />
          <span>{favTimeLabel(note)}</span>
          {note.media.length > 0 && (
            <>
              <span className="dot" />
              <span>
                <Icon name="layers" size={12} /> {note.media.length}
              </span>
            </>
          )}
        </div>
      </div>
      <div className="row-right">
        <StatusPill status={note.status} />
        <span className="row-meta">{NOTE_TYPE[note.noteType]?.label}</span>
      </div>
    </button>
  );
}

function CategoryInput({
  value,
  onChange,
  options,
  placeholder,
  compact = false,
  helper,
  className = '',
}: {
  value: string;
  onChange: (value: string) => void;
  options: string[];
  placeholder?: string;
  compact?: boolean;
  helper?: string;
  className?: string;
}) {
  const [isFocused, setIsFocused] = useState(false);
  const normalized = value.trim().toLowerCase();
  const matches = useMemo(() => {
    const source = normalized
      ? options.filter((option) => option.toLowerCase().includes(normalized))
      : options;
    return source.slice(0, 7);
  }, [normalized, options]);
  const hasExactMatch = Boolean(normalized) && options.some((option) => option.toLowerCase() === normalized);
  const canCreate = Boolean(value.trim()) && !hasExactMatch;
  const showSuggestions = isFocused && (matches.length > 0 || canCreate);

  function pickCategory(category: string) {
    onChange(category);
    setIsFocused(false);
  }

  return (
    <div className={`category-field ${compact ? 'compact' : ''} ${className}`}>
      <input
        aria-autocomplete="list"
        className={`meta-input ${compact ? 'compact-input' : ''}`}
        onBlur={() => setIsFocused(false)}
        onChange={(event) => onChange(event.target.value)}
        onFocus={() => setIsFocused(true)}
        placeholder={placeholder}
        value={value}
      />
      {showSuggestions && (
        <div className="category-suggestions" onMouseDown={(event) => event.preventDefault()}>
          {matches.map((category) => (
            <button className="category-suggestion" key={category} onClick={() => pickCategory(category)} type="button">
              <Icon name="folder" size={13} />
              <span>{category}</span>
            </button>
          ))}
          {canCreate && (
            <button className="category-suggestion create" onClick={() => pickCategory(value.trim())} type="button">
              <Icon name="plus" size={13} />
              <span>保存后新建「{value.trim()}」</span>
            </button>
          )}
        </div>
      )}
      {helper && <span className="field-hint">{helper}</span>}
    </div>
  );
}

function Inspector({
  note,
  overview,
  categoryOptions,
  onStatusChange,
  onClose,
  onTagClick,
  onSaveNote,
  onUpdateNote,
  onDownloadNoteMedia,
}: {
  note: NoteSummary | null;
  overview: LibraryOverview | null;
  categoryOptions: string[];
  onStatusChange: (noteId: string, status: NoteStatus) => void;
  onClose: () => void;
  onTagClick: (tag: string) => void;
  onSaveNote: (noteId: string, userNote: string) => Promise<void> | void;
  onUpdateNote: (input: NoteMetadataUpdateInput) => Promise<void> | void;
  onDownloadNoteMedia: (noteId: string) => Promise<void> | void;
}) {
  const [editingNote, setEditingNote] = useState(false);
  const [draft, setDraft] = useState('');
  const [draftCategory, setDraftCategory] = useState('');
  const [draftTags, setDraftTags] = useState('');
  const [isSavingMeta, setIsSavingMeta] = useState(false);
  const [isSavingNote, setIsSavingNote] = useState(false);
  const [isDownloadingNoteMedia, setIsDownloadingNoteMedia] = useState(false);

  useEffect(() => {
    setEditingNote(false);
    setDraft(note?.userNote || '');
    setDraftCategory(note?.categoryName || '');
    setDraftTags(note?.tags.join(', ') || '');
  }, [note?.id, note?.userNote, note?.categoryName, note?.tags]);

  if (!note) return <div className="insp hidden" />;

  const activeNote = note;
  const flags = noteFlags(note);
  const videoMedia = note.media.find((asset) => asset.mediaType === 'video');
  const videoPreviewSrc = videoMedia ? mediaPreviewSrc(videoMedia, overview) ?? videoMedia.originalUrl ?? null : null;
  const videoPosterSrc = notePosterSource(note, overview);
  const pendingAssets = note.media.filter((asset) => asset.downloadStatus !== 'downloaded').length;
  const richAssetCount = note.media.filter((asset) => asset.mediaType === 'image' || asset.mediaType === 'video' || asset.mediaType === 'file').length;
  const needsMediaDiscovery = !flags.remoteMissing && (richAssetCount === 0 || (!note.content && !note.excerpt));
  const canDownloadNoteMedia = pendingAssets > 0 || needsMediaDiscovery;
  const downloadNoteMediaLabel = needsMediaDiscovery
    ? '补全并下载媒体'
    : `下载当前笔记媒体 · ${pendingAssets}`;

  async function saveMeta() {
    setIsSavingMeta(true);
    try {
      await onUpdateNote({
        noteId: activeNote.id,
        categoryName: draftCategory,
        tags: splitTagInput(draftTags),
      });
    } finally {
      setIsSavingMeta(false);
    }
  }

  async function saveUserNote() {
    setIsSavingNote(true);
    try {
      await onSaveNote(activeNote.id, draft);
      setEditingNote(false);
    } finally {
      setIsSavingNote(false);
    }
  }

  async function downloadNoteMedia() {
    setIsDownloadingNoteMedia(true);
    try {
      await onDownloadNoteMedia(activeNote.id);
    } finally {
      setIsDownloadingNoteMedia(false);
    }
  }

  return (
    <aside className="insp fade-in">
      <div className="insp-cover">
        {videoPreviewSrc ? (
          <div className="insp-video-frame">
            <video
              className="insp-video-player"
              controls
              playsInline
              poster={videoPosterSrc ?? undefined}
              preload="metadata"
              src={videoPreviewSrc}
            />
          </div>
        ) : (
          <Cover
            durationMs={note.noteType === 'video' ? videoMedia?.durationMs : null}
            glyphSize={70}
            note={note}
            overview={overview}
            showMissing
            showPlay
            showType={false}
          />
        )}
        <span className="insp-source">
          <Icon name="bookmark" size={12} stroke={2.4} />
          小红书
        </span>
        <button className="insp-close" onClick={onClose} title="收起详情" type="button">
          <Icon name="x" size={16} />
        </button>
      </div>

      <div className="insp-scroll">
        <div>
          <h2 className="insp-title">{note.title}</h2>
          <div className="insp-byline">
            <Avatar name={note.authorName} size={22} />
            <strong>{note.authorName}</strong>
            <span>· {favTimeLabel(note)}</span>
          </div>
        </div>

        <div className="insp-sec insp-body-primary">
          <h4>正文</h4>
          <p className="body-copy">{note.content || note.excerpt || '等待详情补全'}</p>
        </div>

        {flags.remoteMissing && (
          <div className="alert-bar missing">
            <Icon name="cloudOff" size={17} />
            <div>
              <strong>原帖已从小红书消失</strong>
              {note.unavailableReason || '远端收藏中已找不到该笔记'}，正文与已下载媒体仍保留在本地。
            </div>
          </div>
        )}

        {!flags.remoteMissing && (flags.coverMissing || flags.failed) && (
          <div className="alert-bar local">
            <Icon name="image" size={17} />
            <div>
              <strong>本地媒体未就绪</strong>
              {flags.failed ? '部分资产下载失败，' : ''}有 {flags.pendingMedia} 个素材尚未下载。
            </div>
          </div>
        )}

        <div className="insp-sec">
          <h4>复查状态</h4>
          <div className="status-switch">
            {STATUS_ORDER.map((status) => (
              <button
                className={`status-opt ${status} ${note.status === status ? 'on' : ''}`}
                key={status}
                onClick={() => onStatusChange(note.id, status)}
                type="button"
              >
                <span className="so-ico">
                  <Icon name={STATUS[status].icon} size={14} stroke={2.4} />
                </span>
                {STATUS[status].label}
              </button>
            ))}
          </div>
        </div>

        <div className="insp-sec">
          <h4>分类与标签</h4>
          <div className="meta-edit-grid">
            <label>
              <span>分类</span>
              <CategoryInput
                helper="主分类只能选一个；输入新名字保存后会自动创建。多个维度放到标签里。"
                onChange={setDraftCategory}
                options={categoryOptions}
                placeholder="旅游、美食、待规划..."
                value={draftCategory}
              />
            </label>
            <label>
              <span>标签</span>
              <input
                className="meta-input"
                onChange={(event) => setDraftTags(event.target.value)}
                placeholder="待看, 已看, 过时"
                value={draftTags}
              />
            </label>
          </div>
          <button className="btn btn-ghost btn-sm meta-save" disabled={isSavingMeta} onClick={() => void saveMeta()} type="button">
            <Icon name={isSavingMeta ? 'loader' : 'check'} size={15} className={isSavingMeta ? 'spin' : ''} />
            {isSavingMeta ? '保存中' : '保存分类标签'}
          </button>
        </div>

        <div className="insp-sec">
          <h4>我的批注</h4>
          {editingNote ? (
            <div className="note-editor">
              <textarea
                autoFocus
                className="cookie-field usernote-field"
                onChange={(event) => setDraft(event.target.value)}
                placeholder="写下你对这条收藏的判断：要不要去、是否过时、待确认的事项..."
                value={draft}
              />
              <div className="note-editor-actions">
                <button
                  className="btn btn-primary btn-sm"
                  disabled={isSavingNote}
                  onClick={() => void saveUserNote()}
                  type="button"
                >
                  <Icon name={isSavingNote ? 'loader' : 'check'} size={15} className={isSavingNote ? 'spin' : ''} />
                  {isSavingNote ? '保存中' : '保存'}
                </button>
                <button
                  className="btn btn-quiet btn-sm"
                  onClick={() => {
                    setDraft(note.userNote || '');
                    setEditingNote(false);
                  }}
                  type="button"
                >
                  取消
                </button>
              </div>
            </div>
          ) : (
            <button className={`usernote ${note.userNote ? '' : 'empty'}`} onClick={() => setEditingNote(true)} type="button">
              <span className="un-edit">
                <Icon name="edit" size={14} />
              </span>
              {note.userNote || '点击添加你的批注：要不要去、是否过时、待确认事项...'}
            </button>
          )}
        </div>

        <div className="divider" />

        {note.tags.length > 0 && (
          <div className="insp-sec">
            <h4>标签</h4>
            <div className="tag-cloud">
              {note.tags.map((tag) => (
                <button className="chip tag" key={tag} onClick={() => onTagClick(tag)} type="button">
                  <Icon name="tag" size={12} />
                  {tag}
                </button>
              ))}
            </div>
          </div>
        )}

        <div className="insp-sec">
          <h4>媒体资产 · {note.media.length}</h4>
          {note.media.length === 0 ? (
            <div className="asset-list">
              <p className="body-copy muted-small">这条笔记还没有完整媒体资产，可能只同步到了收藏卡片摘要。</p>
              {canDownloadNoteMedia && (
                <button
                  className="btn btn-ghost btn-sm"
                  disabled={isDownloadingNoteMedia}
                  onClick={() => void downloadNoteMedia()}
                  type="button"
                >
                  <Icon name={isDownloadingNoteMedia ? 'loader' : 'download'} size={15} className={isDownloadingNoteMedia ? 'spin' : ''} />
                  {isDownloadingNoteMedia ? '补全中' : downloadNoteMediaLabel}
                </button>
              )}
            </div>
          ) : (
            <div className="asset-list">
              {note.media.map((asset) => (
                <div className="asset" key={asset.id}>
                  <span className="asset-thumb" style={{ background: coverGrad(note.categoryName, hashSeed(asset.id)) }}>
                    <Icon name={asset.mediaType === 'video' ? 'video' : asset.mediaType === 'file' ? 'fileText' : 'image'} size={16} style={{ color: '#fff' }} />
                  </span>
                  <div className="asset-info">
                    <strong>
                      {MEDIA_TYPE_LABEL[asset.mediaType]}
                      {asset.durationMs ? ` · ${durationFmt(asset.durationMs)}` : ''}
                    </strong>
                    <span>{asset.mimeType || '-'} · {fileSize(asset.sizeBytes)}</span>
                  </div>
                  <DownloadBadge status={asset.downloadStatus} />
                </div>
              ))}
              {canDownloadNoteMedia && (
                <button
                  className="btn btn-ghost btn-sm"
                  disabled={isDownloadingNoteMedia}
                  onClick={() => void downloadNoteMedia()}
                  type="button"
                >
                  <Icon name={isDownloadingNoteMedia ? 'loader' : 'download'} size={15} className={isDownloadingNoteMedia ? 'spin' : ''} />
                  {isDownloadingNoteMedia ? '补全中' : downloadNoteMediaLabel}
                </button>
              )}
            </div>
          )}
        </div>

        <div className="divider" />

        <div className="insp-sec">
          <h4>来源信息</h4>
          <dl className="meta-grid">
            <div>
              <dt>分类</dt>
              <dd>{note.categoryName || '未分类'}</dd>
            </div>
            <div>
              <dt>类型</dt>
              <dd>{NOTE_TYPE[note.noteType]?.label}</dd>
            </div>
            <div>
              <dt>发布时间</dt>
              <dd>{note.publishedAt ? shortDate(note.publishedAt) : '未知'}</dd>
            </div>
            <div>
              <dt>收藏时间</dt>
              <dd>{note.collectedAt ? fullDate(note.collectedAt) : `序 #${note.favoriteOrder ?? '-'}`}</dd>
            </div>
            <div>
              <dt>最近同步</dt>
              <dd>{relTime(note.lastSyncedAt)}</dd>
            </div>
            <div>
              <dt>远端状态</dt>
              <dd>{flags.remoteMissing ? <span className="text-missing">已缺失</span> : '正常'}</dd>
            </div>
            <div>
              <dt>笔记 ID</dt>
              <dd className="mono">{note.sourceNoteId}</dd>
            </div>
            <div>
              <dt>本地库</dt>
              <dd className="mono">{overview?.dbPath ?? '初始化中'}</dd>
            </div>
          </dl>
          <a className="source-link" href={note.sourceUrl} rel="noreferrer" target="_blank">
            <Icon name="externalLink" size={15} />
            在小红书打开原文
          </a>
        </div>
      </div>
    </aside>
  );
}

function SyncView({
  overview,
  savedSession,
  onSessionChange,
  onSynced,
  onSwitchProfile,
}: {
  overview: LibraryOverview | null;
  savedSession: XhsSessionTestResult | null;
  onSessionChange: (session: XhsSessionTestResult | null) => void;
  onSynced: (summary: { message: string }) => void;
  onSwitchProfile: (profileId: string) => Promise<void> | void;
}) {
  const [cookie, setCookie] = useState('');
  const [isTesting, setIsTesting] = useState(false);
  const [isOpeningLogin, setIsOpeningLogin] = useState(false);
  const [isReadingLogin, setIsReadingLogin] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [isSyncingAlbums, setIsSyncingAlbums] = useState(false);
  const [isSyncingFiles, setIsSyncingFiles] = useState(false);
  const [isCancellingSync, setIsCancellingSync] = useState(false);
  const [result, setResult] = useState<XhsSessionTestResult | null>(null);
  const [syncResult, setSyncResult] = useState<XhsFavoriteSyncResult | null>(null);
  const [albumResult, setAlbumResult] = useState<XhsAlbumSyncResult | null>(null);
  const [fileResult, setFileResult] = useState<XhsFavoriteSyncResult | null>(null);
  const [syncProgress, setSyncProgress] = useState<XhsSyncProgress | null>(null);
  const [detailProgress, setDetailProgress] = useState<BatchJobProgress | null>(null);
  const [detailResult, setDetailResult] = useState<BatchJobResult | null>(null);
  const [hasEmbeddedSession, setHasEmbeddedSession] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');
  const [notice, setNotice] = useState('');
  const [showCookie, setShowCookie] = useState(false);
  const [showHelp, setShowHelp] = useState(true);
  const [isEnrichingDetails, setIsEnrichingDetails] = useState(false);

  const postSyncActivePhases = ['post_sync_preparing', 'enriching_details', 'downloading_covers', 'downloading_initial_media'];
  const isPostSyncRunning = Boolean(syncProgress && postSyncActivePhases.includes(syncProgress.phase) && !isSyncing);
  const isBusy = isTesting || isOpeningLogin || isReadingLogin || isSyncing || isSyncingAlbums || isSyncingFiles || isPostSyncRunning || isEnrichingDetails;
  const canCancelSync = isSyncing || isSyncingAlbums || isSyncingFiles || isPostSyncRunning || isEnrichingDetails;
  const canSyncFavorites = hasEmbeddedSession && !isBusy;
  const coverage = overview?.contentCoverage ?? null;
  const coverageTotal = coverage?.totalNotes ?? overview?.notesCount ?? 0;
  const needsDetailCoverage = coverageTotal > 0 && Boolean((coverage?.missingDetailNotes ?? 0) > 0 || (coverage?.missingTagNotes ?? 0) > 0);
  const canEnrichDetails = hasEmbeddedSession && !isBusy && needsDetailCoverage;
  const progressPercent = syncProgress?.progress ?? (syncResult ? 100 : isSyncing ? 5 : 0);
  const detailProgressPercent = detailProgress?.progress ?? (detailResult ? 100 : isEnrichingDetails ? 5 : 0);
  const isSyncComplete =
    syncProgress?.phase === 'completed' ||
    syncProgress?.phase === 'albums_completed' ||
    syncProgress?.phase === 'cancelled' ||
    syncProgress?.phase === 'post_sync_completed' ||
    Boolean((syncResult || albumResult || fileResult) && !isSyncing && !isSyncingAlbums && !isSyncingFiles && !isPostSyncRunning);
  const isProgressIndeterminate = Boolean(syncProgress?.indeterminate && !isSyncComplete);
  const planText = syncProgress?.planned ? `${syncProgress.planned}` : syncResult?.remoteDisplayCount ? `${syncResult.remoteDisplayCount}` : '自动读取';
  const scannedText = syncProgress?.scanned ?? syncProgress?.fetched ?? syncResult?.scanned ?? 0;
  const toSyncText = syncProgress?.toSync ?? (syncResult ? syncResult.fetched : syncProgress?.phase === 'fetching_favorites' ? '读取中' : '-');
  const writtenText = syncProgress?.written ?? (syncResult ? syncResult.inserted + syncResult.updated + syncResult.skipped : 0);
  const stepConnectDone = hasEmbeddedSession || Boolean(result?.ok) || Boolean(notice.includes('登录窗口已打开'));
  const stepVerifyDone = hasEmbeddedSession;
  const stepSyncDone = Boolean(syncResult || albumResult || fileResult);
  const currentStep = !stepConnectDone ? 1 : !stepVerifyDone ? 2 : !stepSyncDone ? 3 : 4;

  function markSyncCancelled() {
    setSyncProgress({
      phase: 'cancelled',
      label: '同步已终止',
      detail: '同步任务已按你的请求停止。已写入的数据会保留在本地库中。',
      planned: syncProgress?.planned ?? 0,
      scanned: syncProgress?.scanned ?? 0,
      fetched: syncProgress?.fetched ?? 0,
      toSync: syncProgress?.toSync ?? null,
      written: syncProgress?.written ?? 0,
      inserted: syncProgress?.inserted ?? 0,
      updated: syncProgress?.updated ?? 0,
      skipped: syncProgress?.skipped ?? 0,
      existingSkipped: syncProgress?.existingSkipped ?? 0,
      progress: 100,
      indeterminate: false,
    });
    setNotice('同步已终止。');
  }

  useEffect(() => {
    if (!isTauriRuntime()) return undefined;
    let cleanup: (() => void) | undefined;
    listen<XhsSyncProgress>('xhs-sync-progress', (event) => {
      setSyncProgress(event.payload);
      setNotice(event.payload.detail);
    })
      .then((unlisten) => {
        cleanup = unlisten;
      })
      .catch(() => undefined);

    return () => {
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return undefined;
    let cleanup: (() => void) | undefined;
    listen<BatchJobProgress>('xhs-detail-progress', (event) => {
      setDetailProgress(event.payload);
      setNotice(event.payload.detail);
    })
      .then((unlisten) => {
        cleanup = unlisten;
      })
      .catch(() => undefined);

    return () => {
      cleanup?.();
    };
  }, []);

  useEffect(() => {
    if (!savedSession) return;
    setResult(savedSession);
    setHasEmbeddedSession(savedSession.ok);
    if (savedSession.ok) {
      setNotice('已加载本地保存的登录态，可以直接快速同步；如同步页要求登录，请重新打开登录窗口。');
    }
  }, [savedSession?.accountId, savedSession?.checkedAt, savedSession?.ok]);

  async function handleOpenLoginWindow() {
    setIsOpeningLogin(true);
    setErrorMessage('');
    setResult(null);
    setHasEmbeddedSession(false);
    setNotice('');
    try {
      await libraryApi.openXhsLoginWindow();
      setNotice('登录窗口已打开。完成登录后回到这里读取并测试登录态。');
      setSyncResult(null);
    } catch (syncError) {
      setErrorMessage(syncError instanceof Error ? syncError.message : '打开登录窗口失败。');
    } finally {
      setIsOpeningLogin(false);
    }
  }

  async function handleReadLoginCookies() {
    setIsReadingLogin(true);
    setErrorMessage('');
    setResult(null);
    setNotice('');
    try {
      const nextResult = await libraryApi.readXhsLoginCookies();
      setResult(nextResult);
      setHasEmbeddedSession(nextResult.ok);
      onSessionChange(nextResult);
      setNotice(nextResult.ok ? '已读取到登录态，账号连接成功。' : '连接未完成，请重新登录或使用手动 Cookie 兜底。');
      setSyncResult(null);
    } catch (syncError) {
      setHasEmbeddedSession(false);
      setErrorMessage(syncError instanceof Error ? syncError.message : '读取登录态失败。');
    } finally {
      setIsReadingLogin(false);
    }
  }

  async function handleTestSession() {
    setIsTesting(true);
    setErrorMessage('');
    setResult(null);
    setNotice('');
    try {
      const nextResult = await libraryApi.testXhsSession(cookie);
      setResult(nextResult);
      setHasEmbeddedSession(nextResult.ok);
      onSessionChange(nextResult);
      setNotice(nextResult.ok ? '手动 Cookie 连接正常。正式同步仍建议使用内置登录窗口。' : '连接未完成，请重新登录或更新 Cookie。');
      setSyncResult(null);
    } catch (syncError) {
      setErrorMessage(syncError instanceof Error ? syncError.message : '连接测试失败。');
    } finally {
      setIsTesting(false);
    }
  }

  async function handleSyncFavorites({ resume = false, fullSync = false }: { resume?: boolean; fullSync?: boolean } = {}) {
    if (!hasEmbeddedSession) {
      setErrorMessage('请先在内置登录窗口完成登录，并点击“读取并测试”。');
      return;
    }
    setIsSyncing(true);
    setErrorMessage('');
    setSyncResult(null);
    setSyncProgress({
      phase: 'preparing',
      label: '准备同步',
      detail: resume
        ? '正在继续完整同步下一批。'
        : fullSync
          ? '正在启动完整同步任务。'
          : '正在启动快速同步任务。',
      planned: 0,
      scanned: 0,
      fetched: 0,
      toSync: null,
      written: 0,
      inserted: 0,
      updated: 0,
      skipped: 0,
      existingSkipped: 0,
      progress: 5,
      indeterminate: true,
    });
    setNotice(
      resume
        ? '正在从当前收藏页位置继续读取。'
        : fullSync
          ? '正在完整同步收藏，这会读取到收藏页末尾。'
          : '正在快速同步最近收藏，遇到已同步条目会停止。',
    );
    try {
      const summary = await libraryApi.syncXhsFavorites({ resume, fullSync });
      setSyncResult(summary);
      setSyncProgress({
        phase: 'completed',
        label: '同步完成',
        detail: summary.message,
        planned: summary.remoteDisplayCount ?? summary.scanned,
        scanned: summary.scanned,
        fetched: summary.scanned,
        toSync: summary.fetched,
        written: summary.inserted + summary.updated + summary.skipped,
        inserted: summary.inserted,
        updated: summary.updated,
        skipped: summary.skipped,
        existingSkipped: summary.existingSkipped,
        progress: 100,
        indeterminate: false,
      });
      setNotice(summary.message);
      onSynced(summary);
    } catch (syncError) {
      const message = syncError instanceof Error ? syncError.message : '同步收藏失败。';
      if (message.includes('同步已终止')) {
        markSyncCancelled();
      } else {
        setErrorMessage(message);
        setNotice('同步没有完成。');
        setSyncProgress(null);
      }
    } finally {
      setIsSyncing(false);
      setIsCancellingSync(false);
    }
  }

  async function handleSyncAlbums() {
    if (!hasEmbeddedSession) {
      setErrorMessage('请先在内置登录窗口完成登录，并点击“读取并测试”。');
      return;
    }
    setIsSyncingAlbums(true);
    setErrorMessage('');
    setAlbumResult(null);
    setSyncProgress({
      phase: 'syncing_albums',
      label: '准备同步专辑',
      detail: '正在读取收藏专辑列表，并尝试进入专辑同步其中的笔记。',
      planned: 0,
      scanned: 0,
      fetched: 0,
      toSync: null,
      written: 0,
      inserted: 0,
      updated: 0,
      skipped: 0,
      existingSkipped: 0,
      progress: 5,
      indeterminate: true,
    });
    setNotice('正在同步小红书收藏专辑。');
    try {
      const summary = await libraryApi.syncXhsAlbums({ maxAlbums: 80, maxNotesPerAlbum: 400 });
      setAlbumResult(summary);
      setSyncProgress({
        phase: 'albums_completed',
        label: '专辑同步完成',
        detail: summary.message,
        planned: summary.albumsScanned,
        scanned: summary.albumsScanned,
        fetched: summary.notesScanned,
        toSync: summary.notesLinked,
        written: summary.notesLinked,
        inserted: summary.notesInserted,
        updated: summary.notesUpdated,
        skipped: summary.skipped,
        existingSkipped: summary.duplicateNotes,
        progress: 100,
        indeterminate: false,
      });
      setNotice(summary.message);
      onSynced(summary);
    } catch (syncError) {
      const message = syncError instanceof Error ? syncError.message : '同步专辑失败。';
      if (message.includes('同步已终止')) {
        markSyncCancelled();
      } else {
        setErrorMessage(message);
        setNotice('专辑同步没有完成。');
        setSyncProgress(null);
      }
    } finally {
      setIsSyncingAlbums(false);
      setIsCancellingSync(false);
    }
  }

  async function handleSyncFiles() {
    if (!hasEmbeddedSession) {
      setErrorMessage('请先在内置登录窗口完成登录，并点击“读取并测试”。');
      return;
    }
    setIsSyncingFiles(true);
    setErrorMessage('');
    setFileResult(null);
    setSyncProgress({
      phase: 'syncing_files',
      label: '准备同步文件',
      detail: '正在读取小红书收藏文件页，并补全可识别的文件媒体资产。',
      planned: 0,
      scanned: 0,
      fetched: 0,
      toSync: null,
      written: 0,
      inserted: 0,
      updated: 0,
      skipped: 0,
      existingSkipped: 0,
      progress: 5,
      indeterminate: true,
    });
    setNotice('正在同步小红书收藏文件。');
    try {
      const summary = await libraryApi.syncXhsFiles({ fullSync: true });
      setFileResult(summary);
      setSyncProgress({
        phase: 'completed',
        label: '文件同步完成',
        detail: summary.message,
        planned: summary.remoteDisplayCount ?? summary.scanned,
        scanned: summary.scanned,
        fetched: summary.scanned,
        toSync: summary.fetched,
        written: summary.inserted + summary.updated + summary.skipped,
        inserted: summary.inserted,
        updated: summary.updated,
        skipped: summary.skipped,
        existingSkipped: summary.existingSkipped,
        progress: 100,
        indeterminate: false,
      });
      setNotice(summary.message);
      onSynced(summary);
    } catch (syncError) {
      const message = syncError instanceof Error ? syncError.message : '同步文件失败。';
      if (message.includes('同步已终止')) {
        markSyncCancelled();
      } else {
        setErrorMessage(message);
        setNotice('文件同步没有完成。');
        setSyncProgress(null);
      }
    } finally {
      setIsSyncingFiles(false);
      setIsCancellingSync(false);
    }
  }

  async function handleContinueDetails() {
    setIsEnrichingDetails(true);
    setErrorMessage('');
    setDetailResult(null);
    setDetailProgress({
      phase: 'preparing',
      label: '准备补全详情',
      detail: '正在继续补全所有待处理收藏的正文、标签和完整媒体地址。',
      planned: 0,
      scanned: 0,
      updated: 0,
      downloaded: 0,
      failed: 0,
      skipped: 0,
      progress: 3,
      indeterminate: true,
    });
    setNotice('正在继续补全正文和标签。');
    try {
      const result = await libraryApi.enrichXhsNoteDetails({});
      setDetailResult(result);
      setNotice(result.message);
      await onSynced(result);
    } catch (syncError) {
      const message = formatErrorMessage(syncError, '补全详情失败。');
      if (message.includes('同步已终止')) {
        markSyncCancelled();
      } else {
        setErrorMessage(message);
        setNotice('详情补全没有完成。');
      }
    } finally {
      setIsEnrichingDetails(false);
      setIsCancellingSync(false);
    }
  }

  async function handleCancelSync() {
    setIsCancellingSync(true);
    setErrorMessage('');
    setNotice('正在终止同步，当前步骤结束后会停止。');
    try {
      await libraryApi.cancelXhsSync();
    } catch (syncError) {
      setErrorMessage(syncError instanceof Error ? syncError.message : '终止同步失败。');
      setIsCancellingSync(false);
    }
  }

  return (
    <div className="sync fade-in">
      <div className="sync-col">
        <div className="panel">
          <div className="panel-head">
            <div className="acct-avatar sync-head-icon">
              <Icon name="key" size={15} style={{ color: '#fff' }} />
            </div>
            <h2>连接并同步你的小红书收藏</h2>
          </div>
          <div className="panel-pad">
            <div className="stepper">
              <Step n={1} state={stepConnectDone ? 'done' : currentStep === 1 ? 'active' : ''} title="连接账号">
                <p>在内置登录窗口里用你自己的账号登录小红书。登录态会保存在本机，过期后再重新登录。</p>
                <div className="step-actions">
                  <button className="btn btn-primary btn-sm" disabled={isBusy} onClick={handleOpenLoginWindow} type="button">
                    <Icon name={isOpeningLogin ? 'loader' : 'login'} size={15} className={isOpeningLogin ? 'spin' : ''} />
                    {isOpeningLogin ? '打开中...' : stepConnectDone ? '重新打开登录窗口' : '打开登录窗口'}
                  </button>
                  {stepConnectDone && (
                    <span className="conn-status">
                      <Icon name="checkCircle" size={14} stroke={2.4} />
                      窗口已打开
                    </span>
                  )}
                </div>
              </Step>

              <Step n={2} state={stepVerifyDone ? 'done' : currentStep === 2 ? 'active' : ''} title="验证登录态">
                <p>读取登录窗口里的 Cookie 并测试，确认能正常访问你的收藏页；成功后下次会优先复用本地登录态。</p>
                <div className="step-actions">
                  <button className="btn btn-ghost btn-sm" disabled={isBusy || !stepConnectDone} onClick={handleReadLoginCookies} type="button">
                    <Icon name={isReadingLogin ? 'loader' : 'cookie'} size={15} className={isReadingLogin ? 'spin' : ''} />
                    {isReadingLogin ? '读取中...' : '读取并测试'}
                  </button>
                </div>
                {hasEmbeddedSession && (
                  <div className="step-card">
                    <div className="conn">
                      <div className="conn-avatar">
                        {result?.avatarUrl ? <img alt="" src={result.avatarUrl} /> : (result?.accountName || '小').slice(0, 1)}
                      </div>
                      <div className="conn-info">
                        <strong>{result?.accountName || '小红书账号'}</strong>
                        <span>
                          <Icon name="user" size={13} />
                          {result?.accountId ?? result?.accountHint ?? '内置登录窗口'}
                        </span>
                      </div>
                      <span className="conn-status">
                        <Icon name="shieldCheck" size={15} stroke={2.3} />
                        已连接
                      </span>
                    </div>
                  </div>
                )}
              </Step>

              <Step n={3} state={stepSyncDone ? 'done' : currentStep === 3 ? 'active' : ''} title="同步收藏">
                <p>自动滚动收藏页、读取每条笔记，并按笔记 ID 去重写入本地库。只读，不会改动你的小红书。</p>
                {coverage && coverageTotal > 0 && (
                  <div className="step-card">
                    <div className="result-grid coverage-grid">
                      <div>
                        <small>本地收藏</small>
                        <strong>{coverageTotal}</strong>
                      </div>
                      <div className={coverage.missingDetailNotes > 0 ? 'warn' : 'hl'}>
                        <small>正文覆盖</small>
                        <strong>{coverage.detailNotes} / {coverageTotal}</strong>
                        <span>{coveragePercent(coverage.detailNotes, coverageTotal)}</span>
                      </div>
                      <div className={coverage.missingTagNotes > 0 ? 'warn' : 'hl'}>
                        <small>标签覆盖</small>
                        <strong>{coverage.taggedNotes} / {coverageTotal}</strong>
                        <span>{coveragePercent(coverage.taggedNotes, coverageTotal)}</span>
                      </div>
                      <div>
                        <small>唯一标签</small>
                        <strong>{coverage.uniqueTags}</strong>
                      </div>
                    </div>
                  </div>
                )}
                <div className="step-actions">
                  <button className="btn btn-primary btn-sm" disabled={!canSyncFavorites} onClick={() => handleSyncFavorites()} type="button">
                    <Icon name={isSyncing ? 'loader' : 'refresh'} size={15} className={isSyncing ? 'spin' : ''} />
                    {isSyncing ? '同步中...' : syncResult ? '快速同步' : '快速同步'}
                  </button>
                  <button className="btn btn-ghost btn-sm" disabled={!canSyncFavorites} onClick={() => handleSyncFavorites({ fullSync: true })} type="button">
                    <Icon name="scan" size={15} />
                    完整同步
                  </button>
                  <button className="btn btn-ghost btn-sm" disabled={!canSyncFavorites} onClick={handleSyncAlbums} type="button">
                    <Icon name={isSyncingAlbums ? 'loader' : 'folder'} size={15} className={isSyncingAlbums ? 'spin' : ''} />
                    同步专辑
                  </button>
                  <button className="btn btn-ghost btn-sm" disabled={!canSyncFavorites} onClick={handleSyncFiles} type="button">
                    <Icon name={isSyncingFiles ? 'loader' : 'fileText'} size={15} className={isSyncingFiles ? 'spin' : ''} />
                    同步文件
                  </button>
                  {needsDetailCoverage && (
                    <button className="btn btn-ghost btn-sm" disabled={!canEnrichDetails} onClick={handleContinueDetails} type="button">
                      <Icon name={isEnrichingDetails ? 'loader' : 'book'} size={15} className={isEnrichingDetails ? 'spin' : ''} />
                      {isEnrichingDetails ? '补全中' : '继续补全正文/标签'}
                    </button>
                  )}
                  {syncResult?.limitReached && (
                    <button className="btn btn-ghost btn-sm" disabled={!canSyncFavorites} onClick={() => handleSyncFavorites({ resume: true, fullSync: true })} type="button">
                      <Icon name="arrowRight" size={15} />
                      继续下一批
                    </button>
                  )}
                  {canCancelSync && (
                    <button className="btn btn-danger btn-sm" disabled={isCancellingSync} onClick={() => void handleCancelSync()} type="button">
                      <Icon name={isCancellingSync ? 'loader' : 'x'} size={15} className={isCancellingSync ? 'spin' : ''} />
                      {isCancellingSync ? '终止中' : '终止同步'}
                    </button>
                  )}
                </div>

                {(syncProgress || isSyncing || errorMessage) && (
                  <div className="step-card">
                    {errorMessage ? (
                      <div className="note-banner warn">
                        <Icon name="alert" size={16} />
                        <div>
                          <strong>同步未完成</strong>
                          {errorMessage}
                        </div>
                      </div>
                    ) : (
                      <>
                        <div className="prog-head">
                          <strong>
                            <Icon
                              name={isSyncComplete ? 'checkCircle' : isSyncing || isPostSyncRunning ? 'loader' : 'refresh'}
                              size={15}
                              className={!isSyncComplete && (isSyncing || isPostSyncRunning) ? 'spin' : ''}
                            />
                            {syncProgress?.label ?? '准备同步'}
                          </strong>
                          <span>{isProgressIndeterminate ? '读取中' : `${Math.max(0, Math.min(100, progressPercent))}%`}</span>
                        </div>
                        <div className={`prog-track ${isProgressIndeterminate ? 'indeterminate' : ''}`} role="progressbar" aria-valuemax={100} aria-valuemin={0} aria-valuenow={isProgressIndeterminate ? undefined : progressPercent}>
                          <div className="prog-fill" style={{ width: `${Math.max(4, Math.min(100, progressPercent))}%` }} />
                        </div>
                        <p className="prog-detail">{syncProgress?.detail ?? '正在启动同步任务。'}</p>
                        <div className="prog-stats">
                          <div>
                            <small>计划读取</small>
                            <strong>{planText}</strong>
                          </div>
                          <div>
                            <small>已扫描</small>
                            <strong>{scannedText}</strong>
                          </div>
                          <div>
                            <small>需同步</small>
                            <strong>{toSyncText}</strong>
                          </div>
                          <div>
                            <small>已写入</small>
                            <strong>{writtenText}</strong>
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                )}

                {(detailProgress || detailResult) && (
                  <div className="step-card">
                    <div className="prog-head">
                      <strong>
                        <Icon
                          name={detailProgress?.phase === 'completed' || detailResult ? 'checkCircle' : isEnrichingDetails ? 'loader' : 'book'}
                          size={15}
                          className={isEnrichingDetails ? 'spin' : ''}
                        />
                        {detailProgress?.label ?? '详情补全'}
                      </strong>
                      <span>{Math.max(0, Math.min(100, detailProgressPercent))}%</span>
                    </div>
                    <div className={`prog-track ${detailProgress?.indeterminate ? 'indeterminate' : ''}`} role="progressbar" aria-valuemax={100} aria-valuemin={0} aria-valuenow={detailProgress?.indeterminate ? undefined : detailProgressPercent}>
                      <div className="prog-fill" style={{ width: `${Math.max(4, Math.min(100, detailProgressPercent))}%` }} />
                    </div>
                    <p className="prog-detail">{detailProgress?.detail ?? detailResult?.message ?? '正在补全详情。'}</p>
                    <div className="prog-stats">
                      <div>
                        <small>计划</small>
                        <strong>{detailProgress?.planned ?? 0}</strong>
                      </div>
                      <div>
                        <small>已处理</small>
                        <strong>{detailProgress?.scanned ?? detailResult?.scanned ?? 0}</strong>
                      </div>
                      <div>
                        <small>补全</small>
                        <strong>{detailProgress?.updated ?? detailResult?.updated ?? 0}</strong>
                      </div>
                      <div>
                        <small>失败</small>
                        <strong>{detailProgress?.failed ?? detailResult?.failed ?? 0}</strong>
                      </div>
                    </div>
                  </div>
                )}
              </Step>

              <Step last n={4} state={stepSyncDone ? 'done' : ''} title="查看结果">
                <p>同步完成后，新增和更新的收藏会出现在「收藏库」，远端缺失的笔记会被标记以便复查。</p>
                {syncResult ? (
                  <div className="step-card">
                    <div className="note-banner ok result-banner">
                      <Icon name="checkCircle" size={16} stroke={2.3} />
                      <div>{syncResult.message}</div>
                    </div>
                    <div className="result-grid">
                      <div>
                        <small>扫描</small>
                        <strong>{syncResult.scanned}</strong>
                      </div>
                      <div className="hl">
                        <small>新增</small>
                        <strong>{syncResult.inserted}</strong>
                      </div>
                      <div className="hl">
                        <small>更新</small>
                        <strong>{syncResult.updated}</strong>
                      </div>
                      <div>
                        <small>已存在跳过</small>
                        <strong>{syncResult.existingSkipped}</strong>
                      </div>
                      <div>
                        <small>其他跳过</small>
                        <strong>{syncResult.skipped}</strong>
                      </div>
                      <div className="warn">
                        <small>远端缺失</small>
                        <strong>{syncResult.remoteMissing}</strong>
                      </div>
                      <div>
                        <small>详情补全</small>
                        <strong>{syncResult.detailsUpdated ?? 0}</strong>
                      </div>
                      <div>
                        <small>封面下载</small>
                        <strong>{syncResult.coversDownloaded ?? 0}</strong>
                      </div>
                      <div>
                        <small>首批媒体</small>
                        <strong>{syncResult.mediaDownloaded ?? 0}</strong>
                      </div>
                    </div>
                  </div>
                ) : !albumResult && !fileResult ? (
                  <p className="muted-small">还没有同步记录。</p>
                ) : null}
                {albumResult && (
                  <div className="step-card">
                    <div className="note-banner ok result-banner">
                      <Icon name="folder" size={16} stroke={2.3} />
                      <div>{albumResult.message}</div>
                    </div>
                    <div className="result-grid">
                      <div>
                        <small>专辑</small>
                        <strong>{albumResult.albumsUpdated}</strong>
                      </div>
                      <div>
                        <small>扫描笔记</small>
                        <strong>{albumResult.notesScanned}</strong>
                      </div>
                      <div className="hl">
                        <small>关联</small>
                        <strong>{albumResult.notesLinked}</strong>
                      </div>
                      <div>
                        <small>新增</small>
                        <strong>{albumResult.notesInserted}</strong>
                      </div>
                      <div>
                        <small>更新</small>
                        <strong>{albumResult.notesUpdated}</strong>
                      </div>
                      <div>
                        <small>已有去重</small>
                        <strong>{albumResult.duplicateNotes}</strong>
                      </div>
                    </div>
                  </div>
                )}
                {fileResult && (
                  <div className="step-card">
                    <div className="note-banner ok result-banner">
                      <Icon name="fileText" size={16} stroke={2.3} />
                      <div>{fileResult.message}</div>
                    </div>
                    <div className="result-grid">
                      <div>
                        <small>扫描</small>
                        <strong>{fileResult.scanned}</strong>
                      </div>
                      <div className="hl">
                        <small>新增</small>
                        <strong>{fileResult.inserted}</strong>
                      </div>
                      <div>
                        <small>更新</small>
                        <strong>{fileResult.updated}</strong>
                      </div>
                      <div>
                        <small>详情补全</small>
                        <strong>{fileResult.detailsUpdated ?? 0}</strong>
                      </div>
                      <div>
                        <small>文件/媒体</small>
                        <strong>{fileResult.mediaDownloaded ?? 0}</strong>
                      </div>
                      <div>
                        <small>失败</small>
                        <strong>{(fileResult.detailsFailed ?? 0) + (fileResult.mediaFailed ?? 0)}</strong>
                      </div>
                    </div>
                  </div>
                )}
              </Step>
            </div>

            {notice && !errorMessage && (
              <div className="note-banner ok notice-banner">
                <Icon name="sparkles" size={15} />
                {notice}
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="sync-col sync-aside">
        <ProfileSwitcher overview={overview} onSwitchProfile={onSwitchProfile} />

        <div className="panel">
          <button className="panel-head as-toggle" onClick={() => setShowHelp((value) => !value)} type="button">
            <Icon name="circleHelp" size={16} />
            <h3>帮助 · 同步是怎么工作的</h3>
            <span className="count">
              <Icon name={showHelp ? 'chevronDown' : 'chevronRight'} size={16} />
            </span>
          </button>
          {showHelp && (
            <div className="panel-pad fade-in">
              <ol className="flow-list">
                <li>登录与读取都在本机的内置窗口完成，登录态优先保存到系统 Keychain，后续会先校验再复用。</li>
                <li>同步是只读的，只从小红书读取你的收藏，不写回、不改动账号。</li>
                <li>以「来源 + 笔记 ID」去重，重复同步不会产生重复收藏。</li>
                <li>专辑同步会把可识别的收藏专辑写成本地集合；同一篇笔记只保存一份，再建立专辑关联。</li>
                <li>读到收藏页末尾自动停止；触发滚动保护时可继续下一批。</li>
                <li>远端已删除的笔记会被标记为「远端缺失」，正文仍在本地保留。</li>
              </ol>
            </div>
          )}
        </div>

        <div className="panel">
          <button className="panel-head as-toggle" onClick={() => setShowCookie((value) => !value)} type="button">
            <Icon name="key" size={16} />
            <h3>手动 Cookie 兜底</h3>
            <span className="count">
              <Icon name={showCookie ? 'chevronDown' : 'chevronRight'} size={16} />
            </span>
          </button>
          {showCookie && (
            <div className="panel-pad fade-in">
              <p className="helper-copy">内置登录窗口不可用时，可从浏览器复制完整 Cookie 粘贴测试。不要把 Cookie 发到聊天里。</p>
              <textarea
                className="cookie-field"
                onChange={(event) => setCookie(event.target.value)}
                placeholder="web_session=...; a1=...; webId=..."
                spellCheck={false}
                value={cookie}
              />
              <div className="cookie-actions">
                <button className="btn btn-ghost btn-sm" disabled={isBusy || !cookie.trim()} onClick={handleTestSession} type="button">
                  <Icon name={isTesting ? 'loader' : 'shieldCheck'} size={15} className={isTesting ? 'spin' : ''} />
                  {isTesting ? '测试中' : '测试连接'}
                </button>
                <span>{cookie.trim() ? `已输入约 ${cookie.length} 个字符` : '等待输入'}</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function ProfileSwitcher({
  overview,
  onSwitchProfile,
}: {
  overview: LibraryOverview | null;
  onSwitchProfile: (profileId: string) => Promise<void> | void;
}) {
  const profiles = overview?.profiles ?? [];
  if (profiles.length === 0) return null;
  return (
    <div className="panel">
      <div className="panel-head">
        <Icon name="user" size={16} />
        <h3>本地账号</h3>
        <span className="count">{profiles.length}</span>
      </div>
      <div className="profile-list">
        {profiles.map((profile) => (
          <button
            className={`profile-row ${profile.isActive ? 'active' : ''}`}
            disabled={profile.isActive}
            key={profile.id}
            onClick={() => onSwitchProfile(profile.id)}
            type="button"
          >
            <ProfileAvatar profile={profile} />
            <span className="profile-main">
              <strong>{profile.displayName || '本机账号'}</strong>
              <span>{profile.sourceAccountId ? `xhs:${profile.sourceAccountId}` : '未绑定小红书'}</span>
            </span>
            {profile.isActive ? <Icon name="checkCircle" size={15} /> : <Icon name="chevronRight" size={15} />}
          </button>
        ))}
      </div>
    </div>
  );
}

function ProfileGate({
  overview,
  onSelect,
  onClose,
  onConnectNew,
}: {
  overview: LibraryOverview;
  onSelect: (profileId: string) => Promise<void> | void;
  onClose: () => void;
  onConnectNew: () => void;
}) {
  return (
    <div className="profile-gate" role="dialog" aria-modal="true" aria-label="选择本地账号">
      <div className="profile-gate-panel">
        <div className="profile-gate-head">
          <span className="acct-avatar sync-head-icon">
            <Icon name="user" size={16} style={{ color: '#fff' }} />
          </span>
          <div>
            <h2>选择本地账号</h2>
            <p>每个本地账号绑定一个小红书账号，并使用独立 SQLite 与媒体目录。</p>
          </div>
        </div>
        <div className="profile-gate-list">
          {overview.profiles.map((profile) => (
            <button
              className={`profile-gate-row ${profile.isActive ? 'active' : ''}`}
              key={profile.id}
              onClick={() => onSelect(profile.id)}
              type="button"
            >
              <ProfileAvatar profile={profile} />
              <span className="profile-main">
                <strong>{profile.displayName}</strong>
                <span>{profile.sourceAccountId ? `xhs:${profile.sourceAccountId}` : '未绑定小红书'}</span>
              </span>
              {profile.isActive && <span className="conn-status">当前</span>}
            </button>
          ))}
        </div>
        <div className="profile-gate-actions">
          <button className="btn btn-ghost" onClick={onClose} type="button">
            <Icon name="check" size={15} />
            继续使用当前
          </button>
          <button className="btn btn-primary" onClick={onConnectNew} type="button">
            <Icon name="login" size={15} />
            连接新账号
          </button>
        </div>
      </div>
    </div>
  );
}

function ProfileAvatar({ profile }: { profile: LocalProfileSummary }) {
  return (
    <span className="profile-avatar">
      {profile.avatarUrl ? <img alt="" src={profile.avatarUrl} /> : (profile.displayName || '本').slice(0, 1)}
    </span>
  );
}

function Step({ n, state, title, children, last = false }: {
  n: number;
  state?: 'done' | 'active' | '';
  title: string;
  children: ReactNode;
  last?: boolean;
}) {
  return (
    <div className={`step ${state ?? ''}`}>
      <div className="step-rail">
        <div className="step-num">{state === 'done' ? <Icon name="check" size={16} stroke={2.6} /> : n}</div>
        {!last && <div className="step-line" />}
      </div>
      <div className="step-body">
        <h3>{title}</h3>
        {children}
      </div>
    </div>
  );
}

type IdleWindow = Window & typeof globalThis & {
  requestIdleCallback?: (callback: () => void, options?: { timeout?: number }) => number;
  cancelIdleCallback?: (handle: number) => void;
};

function scheduleIdleRender(callback: () => void) {
  const idleWindow = window as IdleWindow;
  if (idleWindow.requestIdleCallback && idleWindow.cancelIdleCallback) {
    const handle = idleWindow.requestIdleCallback(callback, { timeout: 180 });
    return () => idleWindow.cancelIdleCallback?.(handle);
  }
  const handle = window.setTimeout(callback, 16);
  return () => window.clearTimeout(handle);
}

function useProgressiveItems<T>(items: T[], initialCount: number, stepCount: number) {
  const safeInitial = Math.max(1, initialCount);
  const safeStep = Math.max(1, stepCount);
  const [state, setState] = useState(() => ({
    count: Math.min(items.length, safeInitial),
    initial: safeInitial,
    items,
  }));
  const visibleCount = state.items === items && state.initial === safeInitial
    ? Math.min(state.count, items.length)
    : Math.min(items.length, safeInitial);

  useEffect(() => {
    const firstCount = Math.min(items.length, safeInitial);
    setState({ count: firstCount, initial: safeInitial, items });
    if (items.length <= firstCount) return undefined;

    let disposed = false;
    let cancelScheduled: (() => void) | undefined;

    function revealMore() {
      if (disposed) return;
      setState((current) => {
        if (current.items !== items) return current;
        const nextCount = Math.min(items.length, current.count + safeStep);
        if (nextCount < items.length && !disposed) {
          cancelScheduled = scheduleIdleRender(revealMore);
        }
        return { ...current, count: nextCount };
      });
    }

    cancelScheduled = scheduleIdleRender(revealMore);
    return () => {
      disposed = true;
      cancelScheduled?.();
    };
  }, [items, safeInitial, safeStep]);

  return {
    hasMore: visibleCount < items.length,
    items: items.slice(0, visibleCount),
    visibleCount,
  };
}

function ProgressiveListTail({ shown, total }: { shown: number; total: number }) {
  if (shown >= total) return null;
  return (
    <div className="progressive-tail" aria-live="polite">
      <Icon name="loader" size={14} className="spin" />
      正在载入 {shown} / {total}
    </div>
  );
}

function MediaView({ notes, overview, onOpenNote, onChanged }: {
  notes: NoteSummary[];
  overview: LibraryOverview | null;
  onOpenNote: (id: string) => void;
  onChanged: (message?: string) => Promise<void> | void;
}) {
  const items = useMemo(() => notes.flatMap((note) => note.media.map((asset) => ({ note, asset }))), [notes]);
  const [typeFilter, setTypeFilter] = useState<MediaAsset['mediaType'] | 'all'>('all');
  const [statusFilter, setStatusFilter] = useState<'all' | 'downloaded' | 'failed' | 'pending'>('all');
  const [size, setSize] = useState<'s' | 'm' | 'l'>('m');
  const [selectedId, setSelectedId] = useState<string | null>(items[0]?.asset.id ?? null);
  const [jobProgress, setJobProgress] = useState<BatchJobProgress | null>(null);
  const [jobResult, setJobResult] = useState<BatchJobResult | null>(null);
  const [jobError, setJobError] = useState('');
  const [busy, setBusy] = useState<'batch-download' | 'asset-download' | ''>('');
  const deferredTypeFilter = useDeferredValue(typeFilter);
  const deferredStatusFilter = useDeferredValue(statusFilter);

  useEffect(() => {
    if (selectedId && items.some((item) => item.asset.id === selectedId)) return;
    setSelectedId(items[0]?.asset.id ?? null);
  }, [items, selectedId]);

  useEffect(() => {
    if (!isTauriRuntime()) return undefined;
    let disposed = false;
    const unlisteners = Promise.all([
      listen<BatchJobProgress>('xhs-detail-progress', (event) => {
        if (!disposed) setJobProgress(event.payload);
      }),
      listen<BatchJobProgress>('media-download-progress', (event) => {
        if (!disposed) setJobProgress(event.payload);
      }),
    ]);
    return () => {
      disposed = true;
      unlisteners.then((items) => items.forEach((unlisten) => unlisten())).catch(() => undefined);
    };
  }, []);

  const summary = useMemo(() => {
    return items.reduce(
      (acc, { asset }) => {
        if (asset.downloadStatus === 'downloaded') acc.downloaded += 1;
        else if (asset.downloadStatus === 'failed') acc.failed += 1;
        else acc.pending += 1;
        return acc;
      },
      { downloaded: 0, pending: 0, failed: 0 },
    );
  }, [items]);

  const shown = useMemo(() => {
    return items.filter(({ asset }) => {
      const typeOk = deferredTypeFilter === 'all' || asset.mediaType === deferredTypeFilter;
      const statusOk =
        deferredStatusFilter === 'all' ||
        (deferredStatusFilter === 'downloaded' && asset.downloadStatus === 'downloaded') ||
        (deferredStatusFilter === 'failed' && asset.downloadStatus === 'failed') ||
        (deferredStatusFilter === 'pending' && ['not_downloaded', 'queued', 'downloading'].includes(asset.downloadStatus));
      return typeOk && statusOk;
    });
  }, [deferredStatusFilter, deferredTypeFilter, items]);
  const progressiveShown = useProgressiveItems(shown, size === 'l' ? 72 : 120, size === 'l' ? 72 : 160);
  const selected = items.find((item) => item.asset.id === selectedId) ?? shown[0] ?? items[0] ?? null;
  const tileSize = { s: 122, m: 158, l: 204 }[size];
  const pendingAssets = summary.pending + summary.failed;
  const progressPercent = jobProgress?.progress ?? 0;
  const isBusy = Boolean(busy);

  async function runDownloadBatch() {
    setBusy('batch-download');
    setJobError('');
    setJobResult(null);
    setJobProgress({
      phase: 'preparing',
      label: '准备下载媒体',
      detail: '正在下载最多 100 个待下载媒体资产。',
      planned: 100,
      scanned: 0,
      updated: 0,
      downloaded: 0,
      failed: 0,
      skipped: 0,
      progress: 3,
      indeterminate: true,
    });
    try {
      const result = await libraryApi.downloadMediaAssets({ limit: 100 });
      setJobResult(result);
      await onChanged(result.message);
    } catch (error) {
      setJobError(error instanceof Error ? error.message : '下载媒体失败。');
    } finally {
      setBusy('');
    }
  }

  async function runAssetDownload(assetId: string) {
    setBusy('asset-download');
    setJobError('');
    setJobResult(null);
    setJobProgress({
      phase: 'preparing',
      label: '准备下载当前媒体',
      detail: '正在下载当前选中的媒体资产。',
      planned: 1,
      scanned: 0,
      updated: 0,
      downloaded: 0,
      failed: 0,
      skipped: 0,
      progress: 3,
      indeterminate: true,
    });
    try {
      const result = await libraryApi.downloadMediaAssets({ assetId, limit: 1 });
      setJobResult(result);
      await onChanged(result.message);
    } catch (error) {
      setJobError(error instanceof Error ? error.message : '下载当前媒体失败。');
    } finally {
      setBusy('');
    }
  }

  if (items.length === 0) {
    return (
      <div className="panel empty-panel">
        <EmptyState
          desc="同步收藏后，每条笔记的封面、图片、视频和可识别文件会出现在这里，像素材库一样按类型与下载状态集中整理。"
          icon="layers"
          title="还没有媒体资产"
        />
      </div>
    );
  }

  return (
    <div className="media-eagle fade-in">
      <div className="me-main">
        <div className="me-toolbar">
          <div className="seg">
            {([
              ['all', '全部'],
              ['image', '图片'],
              ['video', '视频'],
              ['cover', '封面'],
              ['file', '文件'],
            ] as Array<[typeof typeFilter, string]>).map(([key, label]) => (
              <button className={typeFilter === key ? 'active' : ''} key={key} onClick={() => setTypeFilter(key)} type="button">
                {label}
              </button>
            ))}
          </div>
          <div className="seg">
            {([
              ['all', '全部', items.length],
              ['downloaded', '已下载', summary.downloaded],
              ['pending', '待下载', summary.pending],
              ['failed', '失败', summary.failed],
            ] as Array<[typeof statusFilter, string, number]>).map(([key, label, count]) => (
              <button className={statusFilter === key ? 'active' : ''} key={key} onClick={() => setStatusFilter(key)} type="button">
                {label}
                <span className="seg-count">{count}</span>
              </button>
            ))}
          </div>
          <div className="spacer" />
          <span className="count">{shown.length} 个资产</span>
          <div className="seg icons">
            {(['s', 'm', 'l'] as const).map((key) => (
              <button className={size === key ? 'active' : ''} key={key} onClick={() => setSize(key)} title={`缩略图 ${key}`} type="button">
                <Icon name="grid" size={key === 's' ? 12 : key === 'm' ? 15 : 18} />
              </button>
            ))}
          </div>
        </div>

        {(pendingAssets > 0 || jobProgress || jobResult || jobError) && (
          <div className="me-actionbar">
            <Icon name={isBusy ? 'loader' : 'download'} size={15} className={isBusy ? 'spin' : ''} />
            <span>
              {jobProgress?.detail || jobResult?.message || jobError || `${pendingAssets} 个资产待下载。为避免队列过长，每次最多处理 100 个。`}
            </span>
            {jobProgress && (
              <div className={`mini-progress ${jobProgress.indeterminate ? 'indeterminate' : ''}`} aria-hidden="true">
                <div style={jobProgress.indeterminate ? undefined : { width: `${Math.max(4, Math.min(100, progressPercent))}%` }} />
              </div>
            )}
            <button className="btn btn-primary btn-sm" disabled={isBusy || pendingAssets === 0} onClick={runDownloadBatch} type="button">
              <Icon name={busy === 'batch-download' ? 'loader' : 'download'} size={14} className={busy === 'batch-download' ? 'spin' : ''} />
              批量下载最多 100 个
            </button>
          </div>
        )}

        <div className="me-grid-scroll">
          {shown.length === 0 ? (
            <EmptyState desc="换一个类型或下载状态试试。" icon="searchX" title="该筛选下没有资产" />
          ) : (
            <div className="me-grid" style={{ '--tile': `${tileSize}px` } as CSSProperties}>
              {progressiveShown.items.map(({ note, asset }) => (
                <MediaTile
                  asset={asset}
                  isSelected={selected?.asset.id === asset.id}
                  key={asset.id}
                  note={note}
                  onClick={() => setSelectedId(asset.id)}
                  overview={overview}
                />
              ))}
              <ProgressiveListTail shown={progressiveShown.visibleCount} total={shown.length} />
            </div>
          )}
        </div>
      </div>

      <AssetInspector
        isBusy={isBusy}
        item={selected}
        onDownloadAsset={runAssetDownload}
        onOpenNote={onOpenNote}
        overview={overview}
      />
    </div>
  );
}

function MediaTile({ note, asset, isSelected, onClick, overview }: {
  note: NoteSummary;
  asset: MediaAsset;
  isSelected: boolean;
  onClick: () => void;
  overview: LibraryOverview | null;
}) {
  const dl = DL[asset.downloadStatus] || DL.not_downloaded;
  const isVideo = asset.mediaType === 'video';
  const isFile = asset.mediaType === 'file';
  const src = mediaPreviewSrc(asset, overview);
  const posterSrc = isVideo ? notePosterSource(note, overview) : null;
  return (
    <button className={`mtile ${isSelected ? 'selected' : ''}`} onClick={onClick} type="button">
      <div className="mtile-cover">
        {src && (asset.mediaType === 'image' || asset.mediaType === 'cover') ? (
          <img alt="" className="media-thumb-img" loading="lazy" referrerPolicy="no-referrer" src={src} />
        ) : isVideo && posterSrc ? (
          <img alt="" className="media-thumb-img" loading="lazy" referrerPolicy="no-referrer" src={posterSrc} />
        ) : src && asset.mediaType === 'video' ? (
          <video className="media-thumb-img" muted playsInline preload="none" src={src} />
        ) : (
          <Cover durationMs={asset.durationMs} glyphSize={42} note={note} showPlay={isVideo} showType={false} type={isFile ? 'article' : isVideo ? 'video' : 'image'} />
        )}
        <span className="mtile-type">{MEDIA_TYPE_LABEL[asset.mediaType]}</span>
        <span className={`mtile-dl ${dl.cls}`} title={dl.label}>
          <Icon name={dl.icon} size={11} stroke={2.6} className={asset.downloadStatus === 'downloading' ? 'spin' : ''} />
        </span>
      </div>
      <div className="mtile-foot">
        <span className="mtile-title">{note.title}</span>
        <span className="mtile-sub">{asset.sizeBytes ? fileSize(asset.sizeBytes) : dl.label}</span>
      </div>
    </button>
  );
}

function AssetInspector({ item, overview, onOpenNote, onDownloadAsset, isBusy }: {
  item: { note: NoteSummary; asset: MediaAsset } | null;
  overview: LibraryOverview | null;
  onOpenNote: (id: string) => void;
  onDownloadAsset: (assetId: string) => void;
  isBusy: boolean;
}) {
  const [openError, setOpenError] = useState('');

  useEffect(() => {
    setOpenError('');
  }, [item?.asset.id]);

  if (!item) return <aside className="me-insp" />;
  const { note, asset } = item;
  const dl = DL[asset.downloadStatus] || DL.not_downloaded;
  const isVideo = asset.mediaType === 'video';
  const isFile = asset.mediaType === 'file';
  const isDownloaded = asset.downloadStatus === 'downloaded';
  const isFailed = asset.downloadStatus === 'failed';
  const path = mediaAbsolutePath(asset, overview);
  const src = mediaPreviewSrc(asset, overview);
  const previewStyle = mediaAspectStyle(asset);

  async function runOpenPath(reveal = false) {
    setOpenError('');
    try {
      await openLocalPath(path, reveal);
    } catch (error) {
      setOpenError(error instanceof Error ? error.message : reveal ? '显示文件失败。' : '打开文件失败。');
    }
  }

  return (
    <aside className="me-insp">
      <div className="me-preview">
        <div className="media-preview-frame" style={previewStyle}>
          {isDownloaded && isFile ? (
            <div className="me-preview-empty">
              <Icon name="fileText" size={30} />
              <span>文件已下载，可直接打开或在目录中查看</span>
            </div>
          ) : isDownloaded && src ? (
            isVideo ? (
              <video className="media-player" controls playsInline preload="metadata" src={src} />
            ) : (
              <img alt={note.title} className="media-player media-player-img" src={src} />
            )
          ) : isDownloaded ? (
            <div className="me-preview-empty">
              <Icon name={isVideo ? 'video' : isFile ? 'fileText' : 'image'} size={30} />
              <span>本地文件已下载，当前环境无法生成预览地址</span>
            </div>
          ) : (
            <div className="me-preview-empty">
              <Icon name={isFailed ? 'alert' : isVideo ? 'video' : isFile ? 'fileText' : 'image'} size={30} style={isFailed ? { color: 'var(--st-missing-fg)' } : null} />
              <span>{isFailed ? '下载失败' : '本地未下载'}</span>
            </div>
          )}
        </div>
      </div>

      <div className="me-insp-scroll">
        <div>
          <div className="me-insp-type">
            <span className="mtile-type static-type">{MEDIA_TYPE_LABEL[asset.mediaType]}</span>
            <DownloadBadge status={asset.downloadStatus} />
          </div>
          <h3 className="me-insp-title">{note.title}</h3>
          <button className="source-link" onClick={() => onOpenNote(note.id)} type="button">
            <Icon name="externalLink" size={14} />
            查看所属笔记
          </button>
        </div>

        <div className="me-insp-actions">
          {isDownloaded ? (
            <>
              <button className="btn btn-ghost btn-sm" disabled={!path} onClick={() => runOpenPath(false)} type="button">
                <Icon name="maximize" size={15} />
                打开文件
              </button>
              <button className="btn btn-ghost btn-sm" disabled={!path} onClick={() => runOpenPath(true)} type="button">
                <Icon name="folder" size={15} />
                显示文件
              </button>
            </>
          ) : (
            <>
              <button
                className="btn btn-primary btn-sm"
                disabled={isBusy || !asset.originalUrl}
                onClick={() => onDownloadAsset(asset.id)}
                type="button"
              >
                <Icon name={isBusy ? 'loader' : isFailed ? 'rotateCw' : 'download'} size={15} className={isBusy ? 'spin' : ''} />
                下载
              </button>
            </>
          )}
        </div>

        {openError && (
          <div className="note-banner warn compact-banner">
            <Icon name="alert" size={15} />
            {openError}
          </div>
        )}

        <dl className="meta-grid">
          <div>
            <dt>所属笔记</dt>
            <dd>{note.title}</dd>
          </div>
          <div>
            <dt>分类</dt>
            <dd>{note.categoryName || '未分类'}</dd>
          </div>
          <div>
            <dt>类型</dt>
            <dd>{MEDIA_TYPE_LABEL[asset.mediaType]}</dd>
          </div>
          <div>
            <dt>格式</dt>
            <dd>{asset.mimeType || '-'}</dd>
          </div>
          <div>
            <dt>大小</dt>
            <dd>{fileSize(asset.sizeBytes)}</dd>
          </div>
          {isVideo && (
            <div>
              <dt>时长</dt>
              <dd>{durationFmt(asset.durationMs) || '-'}</dd>
            </div>
          )}
          {!isFile && (
            <div>
              <dt>比例</dt>
              <dd>{mediaAspectLabel(asset)}</dd>
            </div>
          )}
          <div>
            <dt>下载状态</dt>
            <dd className={isFailed ? 'text-missing' : isDownloaded ? 'text-ok' : ''}>{dl.label}</dd>
          </div>
        </dl>

        <div>
          <div className="me-insp-pathlabel">本地路径</div>
          <div className="me-insp-path mono">{isDownloaded ? path ?? '已下载，但路径缺失' : '尚未下载到本地'}</div>
        </div>
      </div>
    </aside>
  );
}

type AiTaskName = 'classify_uncategorized' | 'split_category' | 'group_tags' | 'tag_governance';

function clampProgress(value: number) {
  return Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0));
}

function formatErrorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === 'string' && error.trim()) return error;
  return fallback;
}

function isAiCancelMessage(message: string) {
  return message.includes('AI 任务已停止');
}

function makeAiProgress(task: AiTaskName, label: string, detail: string, planned: number): AiJobProgress {
  return {
    task,
    phase: 'starting',
    label,
    detail,
    planned: Math.max(0, planned),
    scanned: 0,
    updated: 0,
    failed: 0,
    skipped: 0,
    progress: planned > 0 ? 2 : 100,
    indeterminate: false,
    error: null,
  };
}

function AiProgressInline({ progress, fallback }: { progress: AiJobProgress | null; fallback: string }) {
  const percent = clampProgress(progress?.progress ?? 2);
  const indeterminate = Boolean(progress?.indeterminate);
  const failed = progress?.phase === 'failed';
  const stopping = progress?.phase === 'cancel_requested';
  const stopped = progress?.phase === 'cancelled';
  return (
    <div className={`ai-run ${failed ? 'is-error' : stopping || stopped ? 'is-stopped' : ''}`}>
      <div className="ai-running">
        <Icon
          name={failed ? 'alert' : stopping ? 'loader' : stopped ? 'x' : 'loader'}
          size={14}
          className={!failed && !stopped ? 'spin' : ''}
        />
        <span>{progress?.detail || fallback}</span>
        {!indeterminate && <b>{failed ? '错误' : stopping ? '停止中' : stopped ? '已停止' : `${Math.round(percent)}%`}</b>}
      </div>
      <div
        className={`prog-track ${indeterminate ? 'indeterminate' : ''}`}
        role="progressbar"
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={indeterminate ? undefined : percent}
      >
        <div className="prog-fill" style={indeterminate ? undefined : { width: `${Math.max(4, percent)}%` }} />
      </div>
    </div>
  );
}

function AiStopButton({
  visible,
  stopRequested,
  onStop,
}: {
  visible: boolean;
  stopRequested: boolean;
  onStop: () => void;
}) {
  if (!visible) return null;
  return (
    <button className="btn btn-ghost btn-sm" disabled={stopRequested} onClick={onStop} type="button">
      <Icon name={stopRequested ? 'loader' : 'x'} size={14} className={stopRequested ? 'spin' : ''} />
      {stopRequested ? '正在停止' : '停止'}
    </button>
  );
}

function AiProgressFooter({
  progress,
  message,
  error,
  running,
  onRetry,
  onStop,
  stopRequested = false,
}: {
  progress: AiJobProgress | null;
  message: string;
  error: string;
  running: boolean;
  onRetry?: () => void;
  onStop?: () => void;
  stopRequested?: boolean;
}) {
  const stopping = progress?.phase === 'cancel_requested';
  const stopped = progress?.phase === 'cancelled';
  const hasError = Boolean(error || progress?.error || progress?.phase === 'failed');
  const detail = hasError ? (progress?.error || progress?.detail || error) : progress?.detail || message;
  if (!progress && !detail && !running) return null;

  const percent = clampProgress(progress?.progress ?? (running ? 2 : detail ? 100 : 0));
  const complete = !hasError && !stopping && !stopped && (progress?.phase === 'completed' || (!running && Boolean(message)));
  const indeterminate = Boolean(progress?.indeterminate && !complete && !hasError && !stopped);
  const label = hasError ? (progress?.label || 'AI 任务失败') : progress?.label || (running ? 'AI 正在运行' : 'AI 任务完成');

  return (
    <div className="ai-form-status">
      <div className={`ai-progress-card ${hasError ? 'is-error' : stopping || stopped ? 'is-stopped' : complete ? 'is-done' : ''}`}>
        <div className="prog-head">
          <strong>
            <Icon
              name={hasError ? 'alert' : stopping ? 'loader' : stopped ? 'x' : complete ? 'checkCircle' : 'loader'}
              size={15}
              className={!hasError && !complete && !stopped ? 'spin' : ''}
            />
            {label}
          </strong>
          <span>{hasError ? '错误' : stopping ? '停止中' : stopped ? '已停止' : indeterminate ? '读取中' : `${Math.round(percent)}%`}</span>
        </div>
        <div
          className={`prog-track ${indeterminate ? 'indeterminate' : ''}`}
          role="progressbar"
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={indeterminate ? undefined : percent}
        >
          <div className="prog-fill" style={indeterminate ? undefined : { width: `${Math.max(4, percent)}%` }} />
        </div>
        {detail && <p className="prog-detail">{detail}</p>}
        {progress && (
          <div className="ai-progress-stats">
            <div><small>计划</small><strong>{progress.planned}</strong></div>
            <div><small>已处理</small><strong>{progress.scanned}</strong></div>
            <div><small>已更新</small><strong>{progress.updated}</strong></div>
            <div><small>失败</small><strong>{progress.failed}</strong></div>
            <div><small>跳过</small><strong>{progress.skipped}</strong></div>
          </div>
        )}
        {((hasError && onRetry) || (running && onStop && !stopped)) && (
          <div className="ai-progress-actions">
            {running && onStop && !stopped && (
              <button className="btn btn-ghost btn-sm" disabled={stopRequested} onClick={onStop} type="button">
                <Icon name={stopRequested ? 'loader' : 'x'} size={14} className={stopRequested ? 'spin' : ''} />
                {stopRequested ? '正在停止' : '停止'}
              </button>
            )}
            {hasError && onRetry && (
              <button className="btn btn-primary btn-sm" onClick={onRetry} type="button">
                <Icon name="refresh" size={14} />
                请重试
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function TagsView({
  notes,
  tags,
  overview,
  aiSettings,
  onCategoryClick,
  onTagClick,
  onChanged,
  onOpenSettings,
}: {
  notes: NoteSummary[];
  tags: TagSummary[];
  overview: LibraryOverview | null;
  aiSettings: AiSettings | null;
  onCategoryClick: (category: string) => void;
  onTagClick: (tag: string) => void;
  onChanged: (message?: string) => Promise<void> | void;
  onOpenSettings: () => void;
}) {
  const [tagQuery, setTagQuery] = useState('');
  const [classifyLimit, setClassifyLimit] = useState(120);
  const [splitSource, setSplitSource] = useState('');
  const [splitTarget, setSplitTarget] = useState('');
  const [splitRule, setSplitRule] = useState('');
  const [isClassifying, setIsClassifying] = useState(false);
  const [isSplitting, setIsSplitting] = useState(false);
  const [isGroupingTags, setIsGroupingTags] = useState(false);
  const [isGoverningTags, setIsGoverningTags] = useState(false);
  const [isClearingTagGroups, setIsClearingTagGroups] = useState(false);
  const [aiMessage, setAiMessage] = useState('');
  const [aiError, setAiError] = useState('');
  const [aiProgress, setAiProgress] = useState<AiJobProgress | null>(null);
  const [aiStopRequested, setAiStopRequested] = useState(false);
  const [classifyResult, setClassifyResult] = useState<AiClassificationResult | null>(null);
  const [splitResult, setSplitResult] = useState<AiClassificationResult | null>(null);
  const [tagGovernanceResult, setTagGovernanceResult] = useState<TagGovernanceSuggestionResult | null>(null);
  const [tagApplyResult, setTagApplyResult] = useState<TagGovernanceApplyResult | null>(null);
  const [selectedRemoveTags, setSelectedRemoveTags] = useState<Set<string>>(() => new Set());
  const [tagGroupFilter, setTagGroupFilter] = useState('all');

  const categories = useMemo(() => {
    const categoryMap = new Map<string, NoteSummary[]>();
    for (const note of notes) {
      const name = note.categoryName || '未分类';
      categoryMap.set(name, [...(categoryMap.get(name) ?? []), note]);
    }
    return [...categoryMap.entries()]
      .map(([name, categoryNotes]) => ({
        name,
        n: categoryNotes.length,
        notes: [...categoryNotes].sort((a, b) => noteTime(b) - noteTime(a)),
      }))
      .sort((a, b) => b.n - a.n);
  }, [notes]);

  const fallbackTags = useMemo<TagSummary[]>(() => {
    const countsMap = new Map<string, number>();
    for (const note of notes) {
      for (const tag of note.tags) countsMap.set(tag, (countsMap.get(tag) ?? 0) + 1);
    }
    return [...countsMap.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([name, count]) => ({ name, count, kind: 'topic', groupName: null }));
  }, [notes]);

  const tagSource = tags.length ? tags : fallbackTags;
  const tagGroups = useMemo(() => {
    const counts = new Map<string, number>();
    for (const tag of tagSource) {
      const group = tag.groupName?.trim() || '未分组';
      counts.set(group, (counts.get(group) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], 'zh-CN'));
  }, [tagSource]);
  const normalizedTagQuery = tagQuery.trim().toLowerCase();
  const filteredTags = tagSource.filter((tag) => {
    const matchesGroup = tagGroupFilter === 'all' || (tag.groupName?.trim() || '未分组') === tagGroupFilter;
    if (!matchesGroup) return false;
    if (!normalizedTagQuery) return true;
    return tag.name.toLowerCase().includes(normalizedTagQuery);
  });
  const groupedTagCount = tagSource.filter((tag) => Boolean(tag.groupName?.trim())).length;

  const maxTag = tagSource[0]?.count || 1;
  const uncategorizedCount = categories.find((category) => category.name === '未分类')?.n ?? 0;
  const aiReady = aiSettingsReady(aiSettings);
  const splitCategory = splitSource || categories.find((category) => category.name !== '未分类')?.name || categories[0]?.name || '';
  const splitCategoryCount = categories.find((category) => category.name === splitCategory)?.n ?? 0;
  const isAiRunning = isClassifying || isSplitting || isGroupingTags || isGoverningTags;
  const isTagActionBusy = isAiRunning || isClearingTagGroups;
  const activeAiTask: AiTaskName | '' = isClassifying
    ? 'classify_uncategorized'
    : isSplitting
      ? 'split_category'
      : isGroupingTags
        ? 'group_tags'
        : isGoverningTags
          ? 'tag_governance'
          : '';
  const visibleAiProgress = aiProgress && (!activeAiTask || aiProgress.task === activeAiTask) ? aiProgress : null;
  const classifyProgress = aiProgress?.task === 'classify_uncategorized' ? aiProgress : null;
  const splitProgress = aiProgress?.task === 'split_category' ? aiProgress : null;
  const tagGovernanceProgress = aiProgress?.task === 'tag_governance' || aiProgress?.task === 'group_tags' ? aiProgress : null;
  const classifyFailed = classifyProgress?.phase === 'failed';
  const splitFailed = splitProgress?.phase === 'failed';
  const tagGovernanceError = tagGovernanceProgress?.phase === 'failed' ? (tagGovernanceProgress.error || tagGovernanceProgress.detail || aiError) : '';

  useEffect(() => {
    if (!isTauriRuntime()) return undefined;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    listen<AiJobProgress>('ai-job-progress', (event) => {
      if (disposed) return;
      setAiProgress(event.payload);
      if (['completed', 'failed', 'cancelled'].includes(event.payload.phase)) {
        setAiStopRequested(false);
      }
      if (event.payload.error || event.payload.phase === 'failed') {
        setAiError(event.payload.error || event.payload.detail);
      } else {
        setAiError('');
        setAiMessage(event.payload.detail);
      }
    })
      .then((unlisten) => {
        cleanup = unlisten;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      cleanup?.();
    };
  }, []);

  function markAiFailed(task: AiTaskName, label: string, detail: string, planned: number) {
    setAiProgress((current) => ({
      task,
      phase: 'failed',
      label,
      detail: current?.task === task && current.phase === 'failed' ? current.error || current.detail || detail : detail,
      planned: current?.task === task ? current.planned : Math.max(0, planned),
      scanned: current?.task === task ? current.scanned : 0,
      updated: current?.task === task ? current.updated : 0,
      failed: current?.task === task ? current.failed || 1 : 1,
      skipped: current?.task === task ? current.skipped : 0,
      progress: current?.task === task ? current.progress : 0,
      indeterminate: false,
      error: current?.task === task && current.phase === 'failed' ? current.error || current.detail || detail : detail,
    }));
  }

  function markAiCancelled(task: AiTaskName, label: string, detail: string, planned: number) {
    setAiError('');
    setAiMessage(detail);
    setAiProgress((current) => ({
      task,
      phase: 'cancelled',
      label,
      detail,
      planned: current?.task === task ? current.planned : Math.max(0, planned),
      scanned: current?.task === task ? current.scanned : 0,
      updated: current?.task === task ? current.updated : 0,
      failed: current?.task === task ? current.failed : 0,
      skipped: current?.task === task ? current.skipped : 0,
      progress: current?.task === task ? current.progress : 0,
      indeterminate: false,
      error: null,
    }));
  }

  async function stopAiTask() {
    if (!isAiRunning || !activeAiTask || aiStopRequested) return;
    const task = activeAiTask;
    setAiStopRequested(true);
    setAiError('');
    setAiMessage('正在停止 AI 任务...');
    setAiProgress((current) => ({
      task,
      phase: 'cancel_requested',
      label: current?.task === task ? current.label : '正在停止 AI 任务',
      detail: '已请求停止，当前 AI 请求返回后会停止写入。',
      planned: current?.task === task ? current.planned : 0,
      scanned: current?.task === task ? current.scanned : 0,
      updated: current?.task === task ? current.updated : 0,
      failed: current?.task === task ? current.failed : 0,
      skipped: current?.task === task ? current.skipped : 0,
      progress: current?.task === task ? current.progress : 0,
      indeterminate: false,
      error: null,
    }));
    try {
      await libraryApi.cancelAiTask(task);
    } catch (error) {
      setAiStopRequested(false);
      setAiError(formatErrorMessage(error, '停止 AI 任务失败'));
    }
  }

  function retryFailedAiTask() {
    if (!aiProgress || aiProgress.phase !== 'failed' || isAiRunning) return;
    if (aiProgress.task === 'classify_uncategorized') {
      void runAiClassify();
    } else if (aiProgress.task === 'split_category') {
      void runAiSplit();
    } else if (aiProgress.task === 'group_tags') {
      void runAiGroupTags();
    } else if (aiProgress.task === 'tag_governance') {
      void runTagGovernance();
    }
  }

  async function runAiClassify() {
    if (isAiRunning) return;
    const planned = Math.min(classifyLimit, Math.max(1, uncategorizedCount));
    setIsClassifying(true);
    setAiStopRequested(false);
    setAiError('');
    setAiMessage('');
    setClassifyResult(null);
    setAiProgress(makeAiProgress('classify_uncategorized', 'AI 自动分类', '正在启动 AI 自动分类。', planned));
    setAiMessage('AI 正在整理未分类收藏...');
    try {
      const result = await libraryApi.aiClassifyUncategorized({ limit: classifyLimit });
      setClassifyResult(result);
      setAiMessage(result.message);
      setAiProgress({
        task: 'classify_uncategorized',
        phase: 'completed',
        label: 'AI 自动分类完成',
        detail: result.message,
        planned: result.scanned,
        scanned: result.scanned,
        updated: result.updated,
        failed: 0,
        skipped: Math.max(0, result.scanned - result.updated),
        progress: 100,
        indeterminate: false,
        error: null,
      });
      await onChanged(result.message);
    } catch (error) {
      const detail = formatErrorMessage(error, 'AI 自动分类失败');
      if (isAiCancelMessage(detail)) {
        markAiCancelled('classify_uncategorized', 'AI 自动分类已停止', detail, planned);
        await onChanged(detail);
      } else {
        setAiError(detail);
        markAiFailed('classify_uncategorized', 'AI 自动分类失败', detail, planned);
      }
    } finally {
      setIsClassifying(false);
      setAiStopRequested(false);
    }
  }

  async function runAiSplit() {
    if (isAiRunning) return;
    const planned = Math.min(classifyLimit, Math.max(1, splitCategoryCount || 1));
    setIsSplitting(true);
    setAiStopRequested(false);
    setAiError('');
    setAiMessage('');
    setSplitResult(null);
    setAiProgress(makeAiProgress('split_category', '拆分当前分类', '正在启动分类筛选。', planned));
    setAiMessage('AI 正在筛选分类...');
    try {
      const result = await libraryApi.aiSplitCategory({
        sourceCategoryName: splitCategory,
        targetCategoryName: splitTarget,
        query: splitRule || splitTarget,
        limit: classifyLimit,
      });
      setSplitResult(result);
      setAiMessage(result.message);
      setAiProgress({
        task: 'split_category',
        phase: 'completed',
        label: '拆分分类完成',
        detail: result.message,
        planned: result.scanned,
        scanned: result.scanned,
        updated: result.updated,
        failed: 0,
        skipped: Math.max(0, result.scanned - result.updated),
        progress: 100,
        indeterminate: false,
        error: null,
      });
      setSplitTarget('');
      setSplitRule('');
      await onChanged(result.message);
    } catch (error) {
      const detail = formatErrorMessage(error, 'AI 分类筛选失败');
      if (isAiCancelMessage(detail)) {
        markAiCancelled('split_category', '拆分分类已停止', detail, planned);
        await onChanged(detail);
      } else {
        setAiError(detail);
        markAiFailed('split_category', '拆分分类失败', detail, planned);
      }
    } finally {
      setIsSplitting(false);
      setAiStopRequested(false);
    }
  }

  async function runAiGroupTags() {
    if (isTagActionBusy) return;
    const planned = Math.max(1, tagSource.length);
    setIsGroupingTags(true);
    setAiStopRequested(false);
    setAiError('');
    setAiMessage('');
    setTagApplyResult(null);
    setAiProgress(makeAiProgress('group_tags', 'AI 分类标签', '正在启动标签分类。', planned));
    try {
      const result = await libraryApi.aiGroupTags({ limit: Math.min(800, Math.max(1, tagSource.length)) });
      setAiMessage(result.message);
      setAiProgress({
        task: 'group_tags',
        phase: 'completed',
        label: '标签分类完成',
        detail: result.message,
        planned: result.scanned,
        scanned: result.scanned,
        updated: result.updated,
        failed: 0,
        skipped: Math.max(0, result.scanned - result.updated),
        progress: 100,
        indeterminate: false,
        error: null,
      });
      await onChanged(result.message);
    } catch (error) {
      const detail = formatErrorMessage(error, 'AI 标签分类失败');
      if (isAiCancelMessage(detail)) {
        markAiCancelled('group_tags', 'AI 标签分类已停止', detail, planned);
        await onChanged(detail);
      } else {
        setAiError(detail);
        markAiFailed('group_tags', 'AI 标签分类失败', detail, planned);
      }
    } finally {
      setIsGroupingTags(false);
      setAiStopRequested(false);
    }
  }

  async function runTagGovernance() {
    if (isTagActionBusy) return;
    const planned = Math.max(1, tagSource.length);
    setIsGoverningTags(true);
    setAiStopRequested(false);
    setAiError('');
    setAiMessage('');
    setTagApplyResult(null);
    setAiProgress(makeAiProgress('tag_governance', '扫描标签问题', '正在扫描无效标签。', planned));
    try {
      const result = await libraryApi.aiSuggestTagMerges({
        limit: Math.min(800, Math.max(1, tagSource.length)),
        useAi: false,
        minConfidence: 0.72,
      });
      setTagGovernanceResult(result);
      setAiMessage(result.message);
      setAiProgress({
        task: 'tag_governance',
        phase: 'completed',
        label: '标签治理扫描完成',
        detail: `发现 ${result.cleanupIssues.length} 个待清理标签。`,
        planned: result.scanned,
        scanned: result.scanned,
        updated: 0,
        failed: 0,
        skipped: result.cleanupIssues.length,
        progress: 100,
        indeterminate: false,
        error: null,
      });
      setSelectedRemoveTags(new Set(result.cleanupIssues.map((issue) => issue.tag)));
    } catch (error) {
      const detail = formatErrorMessage(error, '标签治理失败');
      if (isAiCancelMessage(detail)) {
        markAiCancelled('tag_governance', '标签治理已停止', detail, planned);
      } else {
        setAiError(detail);
        markAiFailed('tag_governance', '标签治理失败', detail, planned);
      }
    } finally {
      setIsGoverningTags(false);
      setAiStopRequested(false);
    }
  }

  async function applyTagGovernance() {
    if (isTagActionBusy) return;
    if (!tagGovernanceResult) return;
    const removeTags = tagGovernanceResult.cleanupIssues
      .filter((issue) => selectedRemoveTags.has(issue.tag))
      .map((issue) => issue.tag);
    if (removeTags.length === 0) {
      setAiError('请先选择要清理的标签。');
      return;
    }
    const ok = window.confirm(`将清理 ${removeTags.length} 个无效标签。不会删除收藏内容，继续吗？`);
    if (!ok) return;
    setIsGoverningTags(true);
    setAiStopRequested(false);
    setAiError('');
    setAiMessage('');
    setAiProgress(makeAiProgress('tag_governance', '应用标签治理', '正在清理无效标签。', removeTags.length));
    try {
      const result = await libraryApi.applyTagGovernance({ removeTags, mergeGroups: [] });
      setTagApplyResult(result);
      setAiMessage(result.message);
      setAiProgress({
        task: 'tag_governance',
        phase: 'completed',
        label: '标签治理完成',
        detail: result.message,
        planned: removeTags.length,
        scanned: removeTags.length,
        updated: result.removedTags + result.mergedTags,
        failed: 0,
        skipped: 0,
        progress: 100,
        indeterminate: false,
        error: null,
      });
      setSelectedRemoveTags(new Set());
      await onChanged(result.message);
      setTagGovernanceResult(null);
    } catch (error) {
      const detail = formatErrorMessage(error, '应用标签治理失败');
      if (isAiCancelMessage(detail)) {
        markAiCancelled('tag_governance', '标签治理已停止', detail, removeTags.length);
      } else {
        setAiError(detail);
        markAiFailed('tag_governance', '应用标签治理失败', detail, removeTags.length);
      }
    } finally {
      setIsGoverningTags(false);
      setAiStopRequested(false);
    }
  }

  function toggleRemoveTag(tag: string) {
    setSelectedRemoveTags((current) => {
      const next = new Set(current);
      if (next.has(tag)) next.delete(tag);
      else next.add(tag);
      return next;
    });
  }

  async function clearTagGroups() {
    if (isTagActionBusy) return;
    if (groupedTagCount === 0) {
      setAiError('');
      setAiMessage('当前没有标签分类需要清空。');
      return;
    }
    if (!window.confirm(`清空当前 ${groupedTagCount} 个标签分类归属？标签和收藏内容会保留。`)) {
      return;
    }
    setIsClearingTagGroups(true);
    setAiError('');
    setAiMessage('正在清空标签分类...');
    setTagGovernanceResult(null);
    setTagApplyResult(null);
    try {
      const result = await libraryApi.clearAiTagGroups();
      setAiMessage(result.message);
      setAiProgress({
        task: 'group_tags',
        phase: 'completed',
        label: '标签分类已清空',
        detail: result.message,
        planned: result.scanned,
        scanned: result.scanned,
        updated: result.cleared,
        failed: 0,
        skipped: Math.max(0, result.scanned - result.cleared),
        progress: 100,
        indeterminate: false,
        error: null,
      });
      setTagGroupFilter('all');
      await onChanged(result.message);
    } catch (error) {
      const detail = formatErrorMessage(error, '清空标签分类失败');
      setAiError(detail);
      setAiProgress({
        task: 'group_tags',
        phase: 'failed',
        label: '清空标签分类失败',
        detail,
        planned: groupedTagCount,
        scanned: 0,
        updated: 0,
        failed: 1,
        skipped: 0,
        progress: 0,
        indeterminate: false,
        error: detail,
      });
    } finally {
      setIsClearingTagGroups(false);
    }
  }

  return (
    <div className="tags-view fade-in">
      <section className="panel ai-board">
        <div className="ai-head">
          <span className="ai-mark">
            <Icon name="cpu" size={22} stroke={1.9} />
          </span>
          <div className="ai-head-text">
            <h2>AI 分类整理</h2>
            <p>用 AI 把堆在一起的收藏整理成可处理的清单：归类、拆分分类。</p>
          </div>
          {aiReady ? (
            <div className="ai-status ready" title={`${aiSettings?.model} @ ${hostOf(aiSettings?.baseUrl)}`}>
              <span className="ai-dot" />
              已就绪
              <span className="ai-model">{aiSettings?.model}</span>
              <span className="ai-host">· {hostOf(aiSettings?.baseUrl)}</span>
            </div>
          ) : (
            <div className="ai-status unset">
              <Icon name="alert" size={14} />
              未配置 AI
              <button className="btn btn-ghost btn-sm" onClick={onOpenSettings} type="button">
                <Icon name="settings" size={14} />
                去设置配置
              </button>
            </div>
          )}
        </div>

        {!aiReady ? (
          <div className="ai-guide">
            <div className="empty-art">
              <Icon name="cpu" size={30} stroke={1.7} />
            </div>
            <h3>先配置 AI 才能自动整理</h3>
            <p>在「设置 · AI 自动整理」里选择服务商、填入 Base URL、模型和 API Key，保存后这里就能一键归类与拆分分类。</p>
            <button className="btn btn-primary" onClick={onOpenSettings} type="button">
              <Icon name="settings" size={16} />
              前往 AI 设置
            </button>
          </div>
        ) : (
          <>
            <div className="ai-task-grid">
              <div className="ai-task">
                <div className="ai-task-head">
                  <span className="ai-task-ico tone-classify">
                    <Icon name="sparkles" size={17} />
                  </span>
                  <div>
                    <h4>AI 自动分类</h4>
                    <p>把未分类收藏批量归入已有或新建分类。</p>
                  </div>
                </div>

                {uncategorizedCount === 0 ? (
                  <div className="ai-task-empty">
                    <Icon name="checkCircle" size={20} stroke={2} />
                    <span>没有未分类收藏，整理得很干净。</span>
                  </div>
                ) : (
                  <>
                    <button className="ai-stat as-link" onClick={() => onCategoryClick('未分类')} type="button">
                      <span className="ai-stat-ico">
                        <Icon name="inbox" size={16} />
                      </span>
                      <span className="v">{uncategorizedCount}</span>
                      <span className="l">条未分类 · 点击查看</span>
                    </button>
                    <label className="ai-field">
                      <span>本次处理数量</span>
                      <input
                        className="text-input ai-input"
                        disabled={isAiRunning}
                        max={Math.max(1, uncategorizedCount)}
                        min={1}
                        onChange={(event) => setClassifyLimit(Math.max(1, Math.min(uncategorizedCount || 1, Number(event.target.value) || 120)))}
                        type="number"
                        value={Math.min(classifyLimit, Math.max(1, uncategorizedCount))}
                      />
                    </label>
                    <button className="btn btn-primary" disabled={isAiRunning} onClick={() => void runAiClassify()} type="button">
                      <Icon name={isClassifying ? 'loader' : 'sparkles'} size={15} className={isClassifying ? 'spin' : ''} />
                      {isClassifying ? '分类中...' : 'AI 自动分类'}
                    </button>
                    <AiStopButton visible={isClassifying} stopRequested={aiStopRequested} onStop={() => void stopAiTask()} />
                  </>
                )}

                {(isClassifying || classifyFailed) && <AiProgressInline progress={classifyProgress} fallback="正在运行，请勿重复点击..." />}
                {classifyFailed && !isClassifying && (
                  <div className="ai-partial ai-error-inline">
                    <Icon name="alert" size={13} />
                    <span>
                      {(classifyProgress?.updated ?? 0) > 0
                        ? `已保存 ${classifyProgress?.updated ?? 0} 条成功结果，未分类数量已刷新；剩余内容可以降低数量后重试。`
                        : '本次没有写入成功结果，可以降低数量或换更稳定的模型后重试。'}
                    </span>
                    <button className="btn-quiet btn-sm ai-retry" disabled={isAiRunning} onClick={() => void runAiClassify()} type="button">重试</button>
                  </div>
                )}
                {classifyResult && !isClassifying && (
                  <div className="ai-run">
                    <div className="ai-result-grid c4">
                      <div><small>已扫描</small><strong>{classifyResult.scanned}</strong></div>
                      <div className="hl"><small>已归类</small><strong>{classifyResult.updated}</strong></div>
                      <div><small>新分类</small><strong>{classifyResult.createdCategories.length}</strong></div>
                      <div><small>建议</small><strong>{classifyResult.assignments.length}</strong></div>
                    </div>
                    {classifyResult.updated < classifyResult.scanned && (
                      <div className="ai-partial">
                        <Icon name="alert" size={13} />
                        <span>已应用高置信度结果，其余条目保留原状态，可稍后重试。</span>
                        <button className="btn-quiet btn-sm ai-retry" disabled={isAiRunning} onClick={() => void runAiClassify()} type="button">重试</button>
                      </div>
                    )}
                  </div>
                )}
              </div>

              <div className="ai-task">
                <div className="ai-task-head">
                  <span className="ai-task-ico tone-split">
                    <Icon name="scissors" size={16} />
                  </span>
                  <div>
                    <h4>拆分当前分类</h4>
                    <p>从一个分类里筛出符合条件的收藏，放进新分类。</p>
                  </div>
                </div>

                {categories.filter((category) => category.name !== '未分类').length === 0 ? (
                  <div className="ai-task-empty">
                    <Icon name="inbox" size={20} />
                    <span>还没有可拆分的分类。</span>
                  </div>
                ) : (
                  <>
                    <div className="ai-split-row">
                      <label className="ai-field">
                        <span>来源分类</span>
                        <select className="text-input ai-input" disabled={isAiRunning} onChange={(event) => setSplitSource(event.target.value)} value={splitCategory}>
                          {categories.filter((category) => category.name !== '未分类').map((category) => (
                            <option key={category.name} value={category.name}>{category.name}（{category.n}）</option>
                          ))}
                        </select>
                      </label>
                      <span className="ai-split-arrow">
                        <Icon name="arrowRight" size={16} />
                      </span>
                      <label className="ai-field">
                        <span>目标分类</span>
                        <input
                          className="text-input ai-input"
                          disabled={isAiRunning}
                          onChange={(event) => setSplitTarget(event.target.value)}
                          placeholder="如：强化学习"
                          value={splitTarget}
                        />
                      </label>
                    </div>
                    <label className="ai-field">
                      <span>筛选条件（自然语言）</span>
                      <textarea
                        className="cookie-field ai-textarea"
                        disabled={isAiRunning}
                        onChange={(event) => setSplitRule(event.target.value)}
                        placeholder="如：筛出讲强化学习 / RLHF / PPO 的笔记"
                        spellCheck={false}
                        value={splitRule}
                      />
                    </label>
                    <label className="ai-field">
                      <span>本次处理数量（来源约 {splitCategoryCount} 条）</span>
                      <input
                        className="text-input ai-input"
                        disabled={isAiRunning}
                        max={Math.max(1, splitCategoryCount)}
                        min={1}
                        onChange={(event) => setClassifyLimit(Math.max(1, Math.min(splitCategoryCount || 1, Number(event.target.value) || 60)))}
                        type="number"
                        value={Math.min(classifyLimit, Math.max(1, splitCategoryCount || 1))}
                      />
                    </label>
                    <button
                      className="btn btn-primary"
                      disabled={isAiRunning || !splitCategory || !splitTarget.trim()}
                      onClick={() => void runAiSplit()}
                      type="button"
                    >
                      <Icon name={isSplitting ? 'loader' : 'scissors'} size={15} className={isSplitting ? 'spin' : ''} />
                      {isSplitting ? '拆分中...' : '拆分当前分类'}
                    </button>
                    <AiStopButton visible={isSplitting} stopRequested={aiStopRequested} onStop={() => void stopAiTask()} />
                  </>
                )}

                {(isSplitting || splitFailed) && <AiProgressInline progress={splitProgress} fallback="正在筛选分类..." />}
                {splitFailed && !isSplitting && (
                  <div className="ai-partial ai-error-inline">
                    <Icon name="alert" size={13} />
                    <span>
                      {(splitProgress?.updated ?? 0) > 0
                        ? `已保存 ${splitProgress?.updated ?? 0} 条命中结果，分类数量已刷新；剩余内容可以稍后重试。`
                        : '本次没有写入命中结果，可以调整筛选条件或降低数量后重试。'}
                    </span>
                    <button className="btn-quiet btn-sm ai-retry" disabled={isAiRunning || !splitTarget.trim()} onClick={() => void runAiSplit()} type="button">重试</button>
                  </div>
                )}
                {splitResult && !isSplitting && (
                  <div className="ai-run">
                    <div className="ai-result-grid c4">
                      <div><small>已扫描</small><strong>{splitResult.scanned}</strong></div>
                      <div className="hl"><small>已移入</small><strong>{splitResult.updated}</strong></div>
                      <div><small>未命中</small><strong>{Math.max(0, splitResult.scanned - splitResult.updated)}</strong></div>
                      <div><small>建议</small><strong>{splitResult.assignments.length}</strong></div>
                    </div>
                  </div>
                )}
              </div>

              <div className="ai-task tag-governance-task">
                <div className="tag-gov-overview">
                  <div className="ai-task-head">
                    <span className="ai-task-ico tone-governance">
                      <Icon name="tags" size={16} />
                    </span>
                    <div>
                      <h4>标签归属</h4>
                      <p>按当前收藏大分类给标签归属，顺手清理无效标签。</p>
                    </div>
                  </div>

                  <div className="ai-result-grid c3">
                    <div><small>标签总数</small><strong>{tagSource.length}</strong></div>
                    <div className="hl"><small>已归属</small><strong>{groupedTagCount}</strong></div>
                    <div><small>待清理</small><strong>{tagGovernanceResult?.cleanupIssues.length ?? '-'}</strong></div>
                  </div>

                  <div className="tag-gov-actions">
                    <button className="btn btn-ghost btn-sm" disabled={isTagActionBusy} onClick={() => void runTagGovernance()} type="button">
                      <Icon name={isGoverningTags ? 'loader' : 'scan'} size={14} className={isGoverningTags ? 'spin' : ''} />
                      扫描问题
                    </button>
                    <button className="btn btn-primary btn-sm" disabled={!aiReady || isTagActionBusy} onClick={() => void runAiGroupTags()} type="button">
                      <Icon name={isGroupingTags ? 'loader' : 'folder'} size={14} className={isGroupingTags ? 'spin' : ''} />
                      AI 分类标签
                    </button>
                    <button className="btn btn-ghost btn-sm" disabled={isTagActionBusy || groupedTagCount === 0} onClick={() => void clearTagGroups()} type="button">
                      <Icon name={isClearingTagGroups ? 'loader' : 'trash'} size={14} className={isClearingTagGroups ? 'spin' : ''} />
                      清空标签分类
                    </button>
                    <AiStopButton visible={isGroupingTags || isGoverningTags} stopRequested={aiStopRequested} onStop={() => void stopAiTask()} />
                  </div>

                  {tagGovernanceError && (
                    <div className="tag-gov-error">
                      <Icon name="alert" size={14} />
                      <span>{tagGovernanceError}</span>
                    </div>
                  )}

                  {tagApplyResult && !isGoverningTags && (
                    <div className="ai-run">
                      <div className="ai-result-grid c3">
                        <div><small>已清理</small><strong>{tagApplyResult.removedTags}</strong></div>
                        <div className="hl"><small>影响笔记</small><strong>{tagApplyResult.affectedNotes}</strong></div>
                        <div><small>状态</small><strong>完成</strong></div>
                      </div>
                    </div>
                  )}

                  {(isGoverningTags || isGroupingTags || isClearingTagGroups) && <AiProgressInline progress={tagGovernanceProgress} fallback={isClearingTagGroups ? '正在清空标签分类...' : '正在整理标签...'} />}
                </div>

                {tagGovernanceResult && (
                  <div className="tag-gov-panel">
                    {tagGovernanceResult.cleanupIssues.length > 0 && (
                      <div className="tag-gov-block">
                        <div className="tag-gov-block-head">
                          <strong>清洗</strong>
                          <span>{selectedRemoveTags.size} / {tagGovernanceResult.cleanupIssues.length}</span>
                        </div>
                        <div className="tag-gov-list compact">
                          {tagGovernanceResult.cleanupIssues.slice(0, 12).map((issue) => (
                            <label className="tag-gov-row" key={issue.tag}>
                              <input checked={selectedRemoveTags.has(issue.tag)} disabled={isTagActionBusy} onChange={() => toggleRemoveTag(issue.tag)} type="checkbox" />
                              <span className="tag-gov-name">{issue.tag}</span>
                              <span className="tag-gov-count">{issue.count}</span>
                            </label>
                          ))}
                        </div>
                      </div>
                    )}

                    <button className="btn btn-primary" disabled={isTagActionBusy || selectedRemoveTags.size === 0} onClick={() => void applyTagGovernance()} type="button">
                      <Icon name={isGoverningTags ? 'loader' : 'check'} size={15} className={isGoverningTags ? 'spin' : ''} />
                      应用选中治理
                    </button>
                  </div>
                )}

                {!tagGovernanceResult && !tagApplyResult && !tagGovernanceError && !isGoverningTags && !isGroupingTags && !isClearingTagGroups && (
                  <div className="tag-gov-empty">
                    <Icon name="tags" size={20} />
                    <strong>按分类整理标签</strong>
                    <span>AI 分类标签会用当前收藏分类作为标签归属；扫描问题只负责清理无效标签。</span>
                  </div>
                )}
              </div>

            </div>

            <AiProgressFooter
              progress={visibleAiProgress}
              message={aiMessage}
              error={aiError}
              running={isAiRunning}
              onRetry={visibleAiProgress?.phase === 'failed' && !isAiRunning ? retryFailedAiTask : undefined}
              onStop={isAiRunning ? () => void stopAiTask() : undefined}
              stopRequested={aiStopRequested}
            />
          </>
        )}
      </section>

      <div className="cat-sec">
        <h2>
          <Icon name="folder" size={16} />
          按分类浏览 <span className="count">{categories.length} 个分类</span>
          <button className="btn btn-ghost btn-sm sec-action disabled-looking" type="button">
            <Icon name="plus" size={15} />
            新建分类待接入
          </button>
        </h2>
        <div className="cat-grid">
          {categories.map(({ name, n, notes: categoryNotes }) => (
            <button className="cat-card" key={name} onClick={() => onCategoryClick(name)} type="button">
              <CategoryCoverStack category={name} count={n} notes={categoryNotes} overview={overview} />
              <div className="cat-body">
                <div className="cat-body-head">
                  <strong>{name}</strong>
                  <span className="cat-arrow">
                    <Icon name="chevronRight" size={16} />
                  </span>
                </div>
                <p className="cat-latest">
                  {categoryNotes[0] ? (
                    <>
                      <span className="lbl">最近 · </span>
                      {categoryNotes[0].title}
                    </>
                  ) : (
                    <span className="lbl">还没有收藏</span>
                  )}
                </p>
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className="tag-cloud-sec">
        <h2>
          <Icon name="tag" size={16} />
          全部标签 <span className="count">{filteredTags.length} / {tagSource.length} 个 · 点击在收藏库里筛选</span>
          <label className="tag-search">
            <Icon name="search" size={14} />
            <input onChange={(event) => setTagQuery(event.target.value)} placeholder="搜索标签" value={tagQuery} />
          </label>
        </h2>
        {tagGroups.length > 0 && (
          <div className="tag-group-filter">
            <button className={tagGroupFilter === 'all' ? 'on' : ''} onClick={() => setTagGroupFilter('all')} type="button">
              全部
              <span>{tagSource.length}</span>
            </button>
            {tagGroups.map(([group, count]) => (
              <button className={tagGroupFilter === group ? 'on' : ''} key={group} onClick={() => setTagGroupFilter(group)} type="button">
                {group}
                <span>{count}</span>
              </button>
            ))}
          </div>
        )}
        <div className="tag-cloud">
          {filteredTags.map((tag) => (
            <button
              className={`chip tag ${tag.count >= maxTag && tag.count > 0 ? 'hot' : ''}`}
              key={tag.name}
              onClick={() => onTagClick(tag.name)}
              type="button"
            >
              <Icon name="tag" size={13} />
              {tag.name}
              <span className="tc-count">{tag.count}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function CategoryCoverStack({ category, count, notes, overview }: {
  category: string;
  count: number;
  notes: NoteSummary[];
  overview: LibraryOverview | null;
}) {
  const covers = notes.slice(0, 3);
  return (
    <div className="cat-stack" aria-hidden="true">
      {covers.length > 0 ? covers.map((note, index) => (
        <span
          className={`cat-stack-cov pos${index}`}
          key={note.id}
          style={{ '--cover-grad': coverGrad(note.categoryName || category, hashSeed(note.id)), zIndex: 3 - index } as CSSProperties}
        >
          <Cover glyphSize={24} note={note} overview={overview} showMissing={false} showPlay={note.noteType === 'video'} showType={false} />
        </span>
      )) : (
        <span className="cat-stack-cov pos1 empty" style={{ background: coverGrad(category, hashSeed(category)) }}>
          <Icon name="bookmark" size={20} style={{ color: 'rgba(255,255,255,.85)' }} />
        </span>
      )}
      <span className="cat-stack-badge">{count}</span>
    </div>
  );
}

function ExportView({ notes, overview }: { notes: NoteSummary[]; overview: LibraryOverview | null }) {
  const [format, setFormat] = useState<'json' | 'csv' | 'markdown'>('json');
  const [opts, setOpts] = useState({ media: true, notes: true, onlyReviewed: false });
  const [exportPath, setExportPath] = useState('');
  const [exportMessage, setExportMessage] = useState('');
  const [exportError, setExportError] = useState('');
  const [exporting, setExporting] = useState(false);
  const [backupPath, setBackupPath] = useState('');
  const [backupMessage, setBackupMessage] = useState('');
  const [backupError, setBackupError] = useState('');
  const [backingUp, setBackingUp] = useState(false);
  const formats: Array<{ key: typeof format; label: string; desc: string }> = [
    { key: 'json', label: 'JSON', desc: '完整结构化数据，适合迁移和二次开发' },
    { key: 'csv', label: 'CSV', desc: '表格视图，适合在表格软件里整理' },
    { key: 'markdown', label: 'Markdown', desc: '可读文档，适合归档和分享清单' },
  ];

  const count = opts.onlyReviewed ? notes.filter((note) => note.status !== 'unread').length : notes.length;

  async function runExport() {
    setExporting(true);
    setExportPath('');
    setExportMessage('');
    setExportError('');
    try {
      const result = await libraryApi.exportLibrary({
        format,
        includeMedia: opts.media,
        includeNotes: opts.notes,
        onlyReviewed: opts.onlyReviewed,
      });
      setExportPath(result.path);
      setExportMessage(result.message);
    } catch (error) {
      setExportError(error instanceof Error ? error.message : '导出失败。');
    } finally {
      setExporting(false);
    }
  }

  async function runBackup() {
    setBackingUp(true);
    setBackupPath('');
    setBackupMessage('');
    setBackupError('');
    try {
      const result = await libraryApi.createLibraryBackup();
      setBackupPath(result.path);
      setBackupMessage(`${result.message} 共 ${result.fileCount} 个文件，${fileSize(result.sizeBytes)}。`);
    } catch (error) {
      setBackupError(error instanceof Error ? error.message : '生成备份失败。');
    } finally {
      setBackingUp(false);
    }
  }

  return (
    <div className="export-view fade-in">
      <div className="export-col">
        <div className="panel">
          <div className="panel-head">
            <Icon name="fileDown" size={16} />
            <h3>选择导出格式</h3>
          </div>
          <div className="panel-pad">
            <div className="fmt-grid">
              {formats.map((item) => (
                <button
                  className={`fmt-card ${format === item.key ? 'on' : ''}`}
                  key={item.key}
                  onClick={() => {
                    setFormat(item.key);
                    setExportPath('');
                    setExportMessage('');
                    setExportError('');
                  }}
                  type="button"
                >
                  <div className="fmt-card-top">
                    <span className="fmt-ico">
                      <Icon name="fileText" size={18} />
                    </span>
                    {format === item.key && (
                      <span className="fmt-check">
                        <Icon name="checkCircle" size={18} stroke={2.3} style={{ color: 'var(--brand)' }} />
                      </span>
                    )}
                  </div>
                  <strong>{item.label}</strong>
                  <span>{item.desc}</span>
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="panel">
          <div className="panel-head">
            <Icon name="settings" size={16} />
            <h3>导出内容</h3>
          </div>
          <div className="panel-pad flush">
            <div className="export-opts">
              <Opt
                desc="导出每条收藏的复查状态、批注和标签"
                label="包含我的批注与状态"
                on={opts.notes}
                onClick={() => setOpts((current) => ({ ...current, notes: !current.notes }))}
              />
              <Opt
                desc="导出图片/视频的下载状态与本地相对路径"
                label="包含媒体索引"
                on={opts.media}
                onClick={() => setOpts((current) => ({ ...current, media: !current.media }))}
              />
              <Opt
                desc="跳过仍是「待看」的笔记"
                label="仅导出已复查的收藏"
                on={opts.onlyReviewed}
                onClick={() => setOpts((current) => ({ ...current, onlyReviewed: !current.onlyReviewed }))}
              />
            </div>
          </div>
        </div>

        {(exportMessage || exportError) && (
          <div className={`note-banner ${exportError ? 'warn' : 'ok'} fade-in`}>
            <Icon name={exportError ? 'alert' : 'checkCircle'} size={16} stroke={2.3} />
            <div>
              <strong>{exportError ? '导出失败' : '导出完成'}</strong>
              {exportError || exportMessage}
            </div>
          </div>
        )}
      </div>

      <div className="export-col">
        <div className="panel">
          <div className="panel-head">
            <Icon name="layers" size={16} />
            <h3>本次导出</h3>
          </div>
          <div className="panel-pad">
            <div className="export-summary">
              <div>
                <div className="es-v">{count}</div>
                <div className="es-l">条收藏将被导出</div>
              </div>
              <div className="export-summary-right">
                <div className="es-v small">{overview?.mediaCount ?? 0}</div>
                <div className="es-l">个媒体资产</div>
              </div>
            </div>
            <button className="btn btn-primary export-run" disabled={exporting || count === 0} onClick={() => void runExport()} type="button">
              <Icon name={exporting ? 'loader' : 'fileDown'} size={16} className={exporting ? 'spin' : ''} />
              {exporting ? '正在生成...' : `导出 ${format.toUpperCase()}`}
            </button>
            {exportPath && (
              <div className="export-path fade-in">
                <span className="mono">{exportPath}</span>
                <div>
                  <button className="btn btn-quiet btn-sm" onClick={() => void openLocalPath(exportPath)} type="button">
                    <Icon name="fileText" size={15} />
                    打开
                  </button>
                  <button className="btn btn-quiet btn-sm" onClick={() => void openLocalPath(exportPath, true)} type="button">
                    <Icon name="folder" size={15} />
                    显示
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="panel">
          <div className="panel-head">
            <Icon name="hardDrive" size={16} />
            <h3>本地源与备份</h3>
          </div>
          <div className="panel-pad flush">
            <div className="set-row">
              <div className="sr-info">
                <strong>SQLite 数据库</strong>
                <span className="mono">{overview?.dbPath ?? '初始化中'}</span>
              </div>
            </div>
            <div className="set-row">
              <div className="sr-info">
                <strong>媒体目录</strong>
                <span className="mono">{overview?.mediaDir ?? '初始化中'}</span>
              </div>
            </div>
            <div className="set-row">
              <div className="sr-info">
                <strong>整库备份</strong>
                <span>打包数据库与媒体为一个 .zip，便于迁移到新设备</span>
              </div>
              <button className="btn btn-primary btn-sm" disabled={backingUp} onClick={() => void runBackup()} type="button">
                <Icon name={backingUp ? 'loader' : 'download'} size={15} className={backingUp ? 'spin' : ''} />
                {backingUp ? '打包中' : '生成备份 Zip'}
              </button>
            </div>
            {(backupMessage || backupError) && (
              <div className={`note-banner ${backupError ? 'warn' : 'ok'} log-status`}>
                <Icon name={backupError ? 'alert' : 'checkCircle'} size={15} />
                <div>
                  <strong>{backupError ? '备份失败' : '备份完成'}</strong>
                  {backupError || backupMessage}
                </div>
              </div>
            )}
            {backupPath && (
              <div className="export-path fade-in">
                <span className="mono">{backupPath}</span>
                <div>
                  <button className="btn btn-quiet btn-sm" onClick={() => void openLocalPath(backupPath)} type="button">
                    <Icon name="fileText" size={15} />
                    打开
                  </button>
                  <button className="btn btn-quiet btn-sm" onClick={() => void openLocalPath(backupPath, true)} type="button">
                    <Icon name="folder" size={15} />
                    显示
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function Opt({ label, desc, on, onClick }: { label: string; desc: string; on: boolean; onClick: () => void }) {
  return (
    <div className="opt-row">
      <div className="or-info">
        <strong>{label}</strong>
        <span>{desc}</span>
      </div>
      <button className={`switch ${on ? 'on' : ''}`} onClick={onClick} type="button">
        <i />
      </button>
    </div>
  );
}

function SettingsView({
  overview,
  notesCount,
  account,
  aiSettings,
  t,
  setTweak,
  onDeleteDatabase,
  onClearMedia,
  onReset,
  onAiSettingsChange,
  onGoSync,
  onSwitchProfile,
  isDeletingDatabase,
  isClearingMedia,
  isResetting,
}: {
  overview: LibraryOverview | null;
  notesCount: number;
  account: AccountSummary;
  aiSettings: AiSettings | null;
  t: Tweaks;
  setTweak: <K extends keyof Tweaks>(key: K, value: Tweaks[K]) => void;
  onDeleteDatabase: () => void;
  onClearMedia: () => void;
  onReset: () => void;
  onAiSettingsChange: (settings: AiSettings) => void;
  onGoSync: () => void;
  onSwitchProfile: (profileId: string) => Promise<void> | void;
  isDeletingDatabase: boolean;
  isClearingMedia: boolean;
  isResetting: boolean;
}) {
  const [aiDraft, setAiDraft] = useState<AiSettingsInput>(() => ({
    provider: aiSettings?.provider ?? 'openai_compatible',
    baseUrl: aiSettings?.baseUrl ?? AI_PRESETS[0].baseUrl,
    model: aiSettings?.model ?? AI_PRESETS[0].model,
    apiKey: '',
    temperature: aiSettings?.temperature ?? 0.2,
    maxTokens: aiSettings?.maxTokens ?? 4096,
  }));
  const [isSavingAi, setIsSavingAi] = useState(false);
  const [isTestingAi, setIsTestingAi] = useState(false);
  const [aiConfigMessage, setAiConfigMessage] = useState('');
  const [aiConfigError, setAiConfigError] = useState('');
  const [showAiKey, setShowAiKey] = useState(false);
  const [promptSettings, setPromptSettings] = useState<AiPromptSettings | null>(null);
  const [promptDrafts, setPromptDrafts] = useState<AiPromptEditorItem[]>([]);
  const [activePromptKey, setActivePromptKey] = useState('classify_uncategorized');
  const [isLoadingPrompts, setIsLoadingPrompts] = useState(false);
  const [isSavingPrompts, setIsSavingPrompts] = useState(false);
  const [promptMessage, setPromptMessage] = useState('');
  const [promptError, setPromptError] = useState('');
  const [logInfo, setLogInfo] = useState<LogFileInfo | null>(null);
  const [logMessage, setLogMessage] = useState('');
  const [logError, setLogError] = useState('');
  const [isClearingLogs, setIsClearingLogs] = useState(false);
  const standardAiPresets = AI_PRESETS.filter((preset) => !preset.custom);
  const customAiPreset = AI_PRESETS.find((preset) => preset.custom) ?? AI_PRESETS[AI_PRESETS.length - 1];
  const activeAiPreset = standardAiPresets.find(
    (preset) => preset.provider === aiDraft.provider && preset.baseUrl === aiDraft.baseUrl && preset.model === aiDraft.model,
  ) ?? standardAiPresets.find((preset) => preset.provider === aiDraft.provider && preset.baseUrl === aiDraft.baseUrl)
    ?? customAiPreset;

  useEffect(() => {
    setAiDraft((current) => ({
      ...current,
      provider: aiSettings?.provider ?? 'openai_compatible',
      baseUrl: aiSettings?.baseUrl ?? AI_PRESETS[0].baseUrl,
      model: aiSettings?.model ?? AI_PRESETS[0].model,
      temperature: aiSettings?.temperature ?? 0.2,
      maxTokens: aiSettings?.maxTokens ?? 4096,
      apiKey: '',
    }));
  }, [aiSettings?.baseUrl, aiSettings?.model, aiSettings?.provider, aiSettings?.temperature, aiSettings?.maxTokens]);

  useEffect(() => {
    let disposed = false;
    setIsLoadingPrompts(true);
    libraryApi.loadAiPromptSettings()
      .then((settings) => {
        if (disposed) return;
        setPromptSettings(settings);
        setPromptDrafts(settings.prompts);
        setActivePromptKey((current) => settings.prompts.some((prompt) => prompt.key === current) ? current : settings.prompts[0]?.key ?? 'classify_uncategorized');
        setPromptError(settings.validationError ?? '');
      })
      .catch((error) => {
        if (!disposed) setPromptError(error instanceof Error ? error.message : '读取 AI Prompt 失败');
      })
      .finally(() => {
        if (!disposed) setIsLoadingPrompts(false);
      });
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    libraryApi.getLogFileInfo()
      .then((info) => {
        if (!disposed) setLogInfo(info);
      })
      .catch((error) => {
        if (!disposed) setLogError(error instanceof Error ? error.message : '读取日志路径失败');
      });
    return () => {
      disposed = true;
    };
  }, []);

  function clearLocalUiCache() {
    try {
      localStorage.removeItem('xhs_design_tweaks');
      localStorage.removeItem('xhs_onboarded');
    } catch {
      // Ignore localStorage failures in locked-down runtimes.
    }
    setTweak('theme', TWEAK_DEFAULTS.theme);
    setTweak('font', TWEAK_DEFAULTS.font);
    setTweak('defaultView', TWEAK_DEFAULTS.defaultView);
  }

  async function clearLogs() {
    if (!window.confirm('清理历史日志文件？当前会话日志会保留，latest.log 会在退出时重新生成。')) {
      return;
    }
    setIsClearingLogs(true);
    setLogMessage('');
    setLogError('');
    try {
      const result = await libraryApi.clearLogFiles();
      setLogInfo(result.info);
      setLogMessage(result.message);
      if (result.failed.length > 0) {
        setLogError(result.failed.join('；'));
      }
    } catch (error) {
      setLogError(error instanceof Error ? error.message : '清理日志失败');
    } finally {
      setIsClearingLogs(false);
    }
  }

  function applyAiPreset(index: number) {
    const preset = AI_PRESETS[index];
    if (!preset) return;
    setAiDraft((current) => ({
      ...current,
      provider: preset.provider,
      baseUrl: preset.baseUrl,
      model: preset.model,
    }));
  }

  async function saveAiConfig(clearApiKey = false) {
    setIsSavingAi(true);
    setAiConfigError('');
    setAiConfigMessage('');
    try {
      const saved = await libraryApi.saveAiSettings({
        ...aiDraft,
        apiKey: clearApiKey ? null : aiDraft.apiKey?.trim() ? aiDraft.apiKey.trim() : null,
        clearApiKey,
        temperature: Number(aiDraft.temperature ?? 0.2),
        maxTokens: Number(aiDraft.maxTokens ?? 4096),
      });
      onAiSettingsChange(saved);
      setAiDraft((current) => ({ ...current, apiKey: '' }));
      setAiConfigMessage(clearApiKey ? 'API Key 已清除。' : saved.hasApiKey ? 'AI 配置已保存，Key 已写入本机安全存储。' : 'AI 配置已保存，尚未保存 API Key。');
    } catch (error) {
      setAiConfigError(error instanceof Error ? error.message : '保存 AI 配置失败');
    } finally {
      setIsSavingAi(false);
    }
  }

  async function testAiConfig() {
    setIsTestingAi(true);
    setAiConfigError('');
    setAiConfigMessage('');
    try {
      const result = await libraryApi.testAiSettings();
      setAiConfigMessage(`${result.message} 当前模型：${result.model}`);
    } catch (error) {
      setAiConfigError(error instanceof Error ? error.message : 'AI 连接测试失败');
    } finally {
      setIsTestingAi(false);
    }
  }

  const activePrompt = promptDrafts.find((prompt) => prompt.key === activePromptKey) ?? promptDrafts[0] ?? null;

  function updateActivePrompt(patch: Partial<AiPromptEditorItem>) {
    if (!activePrompt) return;
    setPromptDrafts((current) => current.map((prompt) => (
      prompt.key === activePrompt.key ? { ...prompt, ...patch } : prompt
    )));
  }

  async function saveAiPrompts() {
    setIsSavingPrompts(true);
    setPromptMessage('');
    setPromptError('');
    try {
      const saved = await libraryApi.saveAiPromptSettings({ prompts: promptDrafts });
      setPromptSettings(saved);
      setPromptDrafts(saved.prompts);
      setPromptMessage('AI Prompt 已保存。');
      setPromptError(saved.validationError ?? '');
    } catch (error) {
      setPromptError(error instanceof Error ? error.message : '保存 AI Prompt 失败');
    } finally {
      setIsSavingPrompts(false);
    }
  }

  async function resetAiPrompts() {
    if (!window.confirm('恢复内置 AI Prompt？当前自定义 prompt 文件会被删除。')) {
      return;
    }
    setIsSavingPrompts(true);
    setPromptMessage('');
    setPromptError('');
    try {
      const reset = await libraryApi.resetAiPromptSettings();
      setPromptSettings(reset);
      setPromptDrafts(reset.prompts);
      setActivePromptKey(reset.prompts[0]?.key ?? 'classify_uncategorized');
      setPromptMessage('AI Prompt 已恢复默认。');
      setPromptError(reset.validationError ?? '');
    } catch (error) {
      setPromptError(error instanceof Error ? error.message : '恢复默认 Prompt 失败');
    } finally {
      setIsSavingPrompts(false);
    }
  }

  return (
    <div className="settings-view fade-in">
      <div className="panel span2">
        <div className="panel-head">
          <Icon name="sparkles" size={16} />
          <h3>外观</h3>
        </div>
        <div className="panel-pad flush">
          <div className="set-row">
            <div className="sr-info">
              <strong>主题</strong>
              <span>淡粉浅色，或暖调暗色资料库</span>
            </div>
            <div className="theme-swatch">
              <button className={`theme-opt ${t.theme === 'light' ? 'on' : ''}`} onClick={() => setTweak('theme', 'light')} title="淡粉浅色" type="button">
                <div className="ts-top light-top" />
                <div className="ts-bot light-bot" />
              </button>
              <button className={`theme-opt ${t.theme === 'dark' ? 'on' : ''}`} onClick={() => setTweak('theme', 'dark')} title="暖调暗色" type="button">
                <div className="ts-top dark-top" />
                <div className="ts-bot dark-bot" />
              </button>
            </div>
          </div>
          <div className="set-row">
            <div className="sr-info">
              <strong>字体方案</strong>
              <span>{t.font === 'sans' ? '现代黑体，干净中性' : t.font === 'serif' ? '杂志宋体，标题更有资料感' : '手账楷体，温暖手写感'}</span>
            </div>
            <div className="choice">
              <button className={t.font === 'sans' ? 'on' : ''} onClick={() => setTweak('font', 'sans')} type="button">现代黑体</button>
              <button className={t.font === 'serif' ? 'on' : ''} onClick={() => setTweak('font', 'serif')} type="button">杂志宋体</button>
              <button className={t.font === 'kai' ? 'on' : ''} onClick={() => setTweak('font', 'kai')} type="button">手账楷体</button>
            </div>
          </div>
        </div>
      </div>

      <div className="panel span2">
        <div className="panel-head">
          <Icon name="cpu" size={16} />
          <h3>AI 自动整理</h3>
          <span className="count">
            {aiSettingsReady(aiSettings) ? (
              <span className="ai-head-ready">
                <span className="ai-dot" />
                {aiSettings?.model} · {hostOf(aiSettings?.baseUrl)}
              </span>
            ) : '未配置'}
          </span>
        </div>

        <div className="ai-form">
          <div className="ai-field full">
            <span>服务商预设</span>
            <div className="ai-presets">
              {AI_PRESETS.map((preset, index) => (
                <button
                  className={`ai-preset ${activeAiPreset.label === preset.label ? 'on' : ''}`}
                  key={preset.label}
                  onClick={() => applyAiPreset(index)}
                  type="button"
                >
                  {preset.label}
                </button>
              ))}
            </div>
          </div>

          <div className="ai-field">
            <span>Provider 类型</span>
            <div className="choice ai-provider">
              <button
                className={aiDraft.provider === 'openai_compatible' ? 'on' : ''}
                onClick={() => setAiDraft((current) => ({ ...current, provider: 'openai_compatible' }))}
                type="button"
              >
                OpenAI 兼容
              </button>
              <button
                className={aiDraft.provider === 'claude' ? 'on' : ''}
                onClick={() => setAiDraft((current) => ({ ...current, provider: 'claude' }))}
                type="button"
              >
                Claude
              </button>
            </div>
          </div>

          <div className="ai-field">
            <span>Model</span>
            <input
              className="text-input"
              onChange={(event) => setAiDraft((current) => ({ ...current, model: event.target.value }))}
              placeholder="如 deepseek-chat"
              spellCheck={false}
              value={aiDraft.model}
            />
          </div>

          <div className="ai-field full">
            <span>Base URL</span>
            <input
              className="text-input mono-input"
              onChange={(event) => setAiDraft((current) => ({ ...current, baseUrl: event.target.value }))}
              placeholder="https://api.example.com/v1"
              spellCheck={false}
              value={aiDraft.baseUrl}
            />
          </div>

          <div className="ai-field full">
            <span className="ai-key-label">
              <span className="ai-key-text">API Key</span>
              {aiSettings?.hasApiKey && (
                <span className="ai-key-saved">
                  <Icon name="checkCircle" size={12} stroke={2.4} />
                  Key 已保存
                </span>
              )}
            </span>
            <div className="ai-key-row">
              <input
                autoComplete="off"
                className="text-input mono-input"
                onChange={(event) => setAiDraft((current) => ({ ...current, apiKey: event.target.value }))}
                placeholder={aiSettings?.hasApiKey ? '留空则不修改已保存的 Key' : 'sk-...'}
                spellCheck={false}
                type={showAiKey ? 'text' : 'password'}
                value={aiDraft.apiKey ?? ''}
              />
              <button className="btn btn-icon btn-ghost" onClick={() => setShowAiKey((current) => !current)} title={showAiKey ? '隐藏' : '显示'} type="button">
                <Icon name={showAiKey ? 'eyeOff' : 'eye'} size={16} />
              </button>
              {aiSettings?.hasApiKey && (
                <button className="btn btn-ghost btn-sm" disabled={isSavingAi} onClick={() => void saveAiConfig(true)} type="button">
                  清除
                </button>
              )}
            </div>
          </div>

          <div className="ai-field">
            <span>Temperature · {Number(aiDraft.temperature ?? 0.2).toFixed(1)}</span>
            <div className="ai-slider-row">
              <input
                className="ai-range"
                max={1.2}
                min={0}
                onChange={(event) => setAiDraft((current) => ({ ...current, temperature: Number(event.target.value) }))}
                step={0.1}
                type="range"
                value={aiDraft.temperature ?? 0.2}
              />
              <span className="ai-slider-val">{Number(aiDraft.temperature ?? 0.2).toFixed(1)}</span>
            </div>
          </div>

          <div className="ai-field">
            <span>Max tokens</span>
            <input
              className="text-input"
              max={16000}
              min={512}
              onChange={(event) => setAiDraft((current) => ({ ...current, maxTokens: Number(event.target.value) }))}
              step={256}
              type="number"
              value={aiDraft.maxTokens ?? 4096}
            />
          </div>

          <div className="ai-doc-row full">
            {activeAiPreset.docsUrl ? (
              <button className="btn btn-quiet btn-sm" onClick={() => void openExternalUrl(activeAiPreset.docsUrl!)} type="button">
                <Icon name="fileText" size={15} />
                {activeAiPreset.label} API 文档
              </button>
            ) : (
              <span className="ai-doc-note">
                自定义端点会按上方 Provider 类型调用；填写服务商提供的 Base URL、Model 和 API Key 后保存。
              </span>
            )}
            {activeAiPreset.pricingUrl && (
              <button className="btn btn-quiet btn-sm" onClick={() => void openExternalUrl(activeAiPreset.pricingUrl!)} type="button">
                <Icon name="creditCard" size={15} />
                定价 / 余额
              </button>
            )}
          </div>
        </div>

        {(aiConfigMessage || aiConfigError) && (
          <div className="ai-form-status">
            <div className={`note-banner ${aiConfigError ? 'warn' : 'ok'}`}>
              <Icon name={aiConfigError ? 'alert' : 'checkCircle'} size={15} />
              <div>{aiConfigError || aiConfigMessage}</div>
            </div>
          </div>
        )}

        <div className="ai-form-foot">
          <span className="ai-foot-hint">
            <Icon name="key" size={13} />
            Key 只保存在本机，不会上传。
          </span>
          <div className="ai-foot-actions">
            <button className="btn btn-ghost" disabled={isTestingAi || !aiSettings?.hasApiKey} onClick={() => void testAiConfig()} type="button">
              <Icon name={isTestingAi ? 'loader' : 'shieldCheck'} size={16} className={isTestingAi ? 'spin' : ''} />
              测试 AI 连接
            </button>
            <button className="btn btn-primary" disabled={isSavingAi} onClick={() => void saveAiConfig()} type="button">
              <Icon name={isSavingAi ? 'loader' : 'check'} size={16} className={isSavingAi ? 'spin' : ''} />
              保存 AI 设置
            </button>
          </div>
        </div>
      </div>

      <div className="panel span2 prompt-panel">
        <div className="panel-head">
          <Icon name="fileText" size={16} />
          <h3>AI Prompt</h3>
          <span className="count">{promptSettings?.isCustom ? '自定义' : '默认'}</span>
          <button
            className="btn btn-quiet btn-sm sec-action"
            disabled={!promptSettings?.isCustom || !promptSettings?.path}
            onClick={() => void openLocalPath(promptSettings?.path ?? null, true)}
            type="button"
          >
            <Icon name="folderInput" size={15} />
            显示文件
          </button>
        </div>
        <div className="prompt-body">
          <div className="prompt-tabs" role="tablist" aria-label="AI Prompt 任务">
            {promptDrafts.map((prompt) => (
              <button
                aria-selected={prompt.key === activePrompt?.key}
                className={prompt.key === activePrompt?.key ? 'on' : ''}
                key={prompt.key}
                onClick={() => setActivePromptKey(prompt.key)}
                role="tab"
                type="button"
              >
                {prompt.label}
              </button>
            ))}
          </div>

          {isLoadingPrompts ? (
            <div className="prompt-empty">
              <Icon name="loader" size={16} className="spin" />
              <span>正在读取 Prompt</span>
            </div>
          ) : activePrompt ? (
            <div className="prompt-editor">
              <label className="ai-field full">
                <span>System</span>
                <textarea
                  className="cookie-field prompt-textarea"
                  onChange={(event) => updateActivePrompt({ system: event.target.value })}
                  spellCheck={false}
                  value={activePrompt.system}
                />
              </label>

              {activePrompt.user !== null && activePrompt.user !== undefined && (
                <label className="ai-field full">
                  <span>User</span>
                  <textarea
                    className="cookie-field prompt-textarea"
                    onChange={(event) => updateActivePrompt({ user: event.target.value })}
                    spellCheck={false}
                    value={activePrompt.user ?? ''}
                  />
                </label>
              )}

              {activePrompt.task !== null && activePrompt.task !== undefined && (
                <label className="ai-field full">
                  <span>Task</span>
                  <textarea
                    className="cookie-field prompt-textarea prompt-textarea-tall"
                    onChange={(event) => updateActivePrompt({ task: event.target.value })}
                    spellCheck={false}
                    value={activePrompt.task ?? ''}
                  />
                </label>
              )}

              {(activePrompt.key === 'tag_merge_suggestions' || activePrompt.rules.length > 0) && (
                <label className="ai-field full">
                  <span>Rules</span>
                  <textarea
                    className="cookie-field prompt-textarea"
                    onChange={(event) => updateActivePrompt({
                      rules: event.target.value
                        .split('\n')
                        .map((line) => line.trim())
                        .filter(Boolean),
                    })}
                    spellCheck={false}
                    value={activePrompt.rules.join('\n')}
                  />
                </label>
              )}

              {activePrompt.schemaText !== null && activePrompt.schemaText !== undefined && (
                <label className="ai-field full">
                  <span>{activePrompt.schemaKind === 'return_json_shape' ? 'Return JSON shape' : 'Output schema'}</span>
                  <textarea
                    className="cookie-field prompt-textarea prompt-schema"
                    onChange={(event) => updateActivePrompt({ schemaText: event.target.value })}
                    spellCheck={false}
                    value={activePrompt.schemaText ?? ''}
                  />
                </label>
              )}
            </div>
          ) : (
            <div className="prompt-empty">
              <Icon name="alert" size={16} />
              <span>没有可编辑的 Prompt</span>
            </div>
          )}
        </div>

        {(promptMessage || promptError || promptSettings?.validationError) && (
          <div className="ai-form-status">
            <div className={`note-banner ${promptError || promptSettings?.validationError ? 'warn' : 'ok'}`}>
              <Icon name={promptError || promptSettings?.validationError ? 'alert' : 'checkCircle'} size={15} />
              <div>{promptError || promptSettings?.validationError || promptMessage}</div>
            </div>
          </div>
        )}

        <div className="ai-form-foot">
          <span className="ai-foot-hint">
            <Icon name="fileText" size={13} />
            {promptSettings?.path ?? 'Prompt 文件路径读取中'}
          </span>
          <div className="ai-foot-actions">
            <button className="btn btn-ghost" disabled={isSavingPrompts || isLoadingPrompts} onClick={() => void resetAiPrompts()} type="button">
              <Icon name="rotateCw" size={16} />
              恢复默认
            </button>
            <button className="btn btn-primary" disabled={isSavingPrompts || isLoadingPrompts || promptDrafts.length === 0} onClick={() => void saveAiPrompts()} type="button">
              <Icon name={isSavingPrompts ? 'loader' : 'check'} size={16} className={isSavingPrompts ? 'spin' : ''} />
              保存 Prompt
            </button>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">
          <Icon name="user" size={16} />
          <h3>账号与登录态</h3>
        </div>
        <div className="panel-pad flush">
          <div className="set-row">
            <span className="acct-avatar" style={{ width: 38, height: 38, background: `linear-gradient(140deg, ${account.avatarTone[0]}, ${account.avatarTone[1]})` }}>
              {account.avatarUrl ? <img alt="" src={account.avatarUrl} /> : account.nickname.slice(0, 1)}
            </span>
            <div className="sr-info">
              <strong>{account.nickname}</strong>
              <span>{account.handle} · {account.connected ? '本地登录态已保存' : '未连接小红书'}</span>
            </div>
            <button className="btn btn-ghost btn-sm" onClick={onGoSync} type="button">
              <Icon name="login" size={15} />
              去同步页
            </button>
          </div>
          <div className="set-row">
            <div className="sr-info">
              <strong>最近同步</strong>
              <span>{fullDate(account.lastSyncedAt)} · {relTime(account.lastSyncedAt)}</span>
            </div>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">
          <Icon name="layers" size={16} />
          <h3>本地账号库 <span className="count">{overview?.profiles.length ?? 0} 个</span></h3>
        </div>
        <div className="panel-pad flush profile-settings">
          {(overview?.profiles ?? []).map((profile) => (
            <div className="set-row" key={profile.id}>
              <ProfileAvatar profile={profile} />
              <div className="sr-info">
                <strong>{profile.displayName}</strong>
                <span>{profile.sourceAccountId ? `xhs:${profile.sourceAccountId}` : '未绑定小红书'} · {profile.sessionStatus}</span>
              </div>
              <button
                className="btn btn-ghost btn-sm"
                disabled={profile.isActive}
                onClick={() => onSwitchProfile(profile.id)}
                type="button"
              >
                <Icon name={profile.isActive ? 'checkCircle' : 'user'} size={15} />
                {profile.isActive ? '当前' : '切换'}
              </button>
            </div>
          ))}
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">
          <Icon name="hardDrive" size={16} />
          <h3>本地存储 <span className="count">{notesCount} 条</span></h3>
          <button className="btn btn-ghost btn-sm sec-action disabled-looking" type="button">
            <Icon name="folderInput" size={15} />
            迁移目录待接入
          </button>
        </div>
        <div className="panel-pad flush">
          <div className="set-row">
            <div className="sr-info">
              <strong>App 数据目录</strong>
              <span className="mono">{overview?.appDataDir ?? '初始化中'}</span>
            </div>
            <button className="btn btn-quiet btn-sm" disabled={!overview?.appDataDir} onClick={() => void openLocalPath(overview?.appDataDir ?? null)} type="button">
              <Icon name="folder" size={15} />
              打开
            </button>
          </div>
          <div className="set-row">
            <div className="sr-info">
              <strong>SQLite</strong>
              <span className="mono">{overview?.dbPath ?? '初始化中'}</span>
            </div>
          </div>
          <div className="set-row">
            <div className="sr-info">
              <strong>媒体目录</strong>
              <span className="mono">{overview?.mediaDir ?? '初始化中'}</span>
            </div>
            <button className="btn btn-quiet btn-sm" disabled={!overview?.mediaDir} onClick={() => void openLocalPath(overview?.mediaDir ?? null)} type="button">
              <Icon name="folder" size={15} />
              打开
            </button>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">
          <Icon name="fileText" size={16} />
          <h3>诊断日志</h3>
          <button className="btn btn-quiet btn-sm sec-action" disabled={!logInfo?.logDir} onClick={() => void openLocalPath(logInfo?.logDir ?? null)} type="button">
            <Icon name="folder" size={15} />
            打开目录
          </button>
        </div>
        <div className="panel-pad flush">
          <div className="set-row">
            <div className="sr-info">
              <strong>当前会话</strong>
              <span className="mono">{logInfo?.currentLogPath ?? '读取中'}</span>
            </div>
            <button className="btn btn-quiet btn-sm" disabled={!logInfo?.currentLogExists} onClick={() => void openLocalPath(logInfo?.currentLogPath ?? null, true)} type="button">
              <Icon name="folderInput" size={15} />
              显示
            </button>
          </div>
          <div className="set-row">
            <div className="sr-info">
              <strong>latest.log</strong>
              <span className="mono">{logInfo?.latestLogPath ?? '正常退出后生成'}</span>
            </div>
            <button className="btn btn-quiet btn-sm" disabled={!logInfo?.latestLogExists} onClick={() => void openLocalPath(logInfo?.latestLogPath ?? null, true)} type="button">
              <Icon name="folderInput" size={15} />
              显示
            </button>
          </div>
          {(logMessage || logError) && (
            <div className={`note-banner ${logError ? 'warn' : 'ok'} log-status`}>
              <Icon name={logError ? 'alert' : 'checkCircle'} size={15} />
              <div>{logError || logMessage}</div>
            </div>
          )}
        </div>
      </div>

      <div className="panel danger-zone span2">
        <div className="panel-head">
          <Icon name="trash" size={16} />
          <h3>危险区</h3>
        </div>
        <div className="danger-body">
          <p>这些操作只影响本机数据，不会修改你的小红书账号或远端收藏。删除数据库会保留媒体文件，清空媒体会保留收藏记录，整库清空会同时清空两者。</p>
          <div className="danger-grid">
            <div className="danger-item">
              <div>
                <strong>删除数据库</strong>
                <span>删除当前本地账号的 SQLite 与同步记录，保留媒体文件。</span>
              </div>
              <button className="btn btn-danger btn-sm" disabled={isDeletingDatabase} onClick={onDeleteDatabase} type="button">
                <Icon name={isDeletingDatabase ? 'loader' : 'trash'} size={15} className={isDeletingDatabase ? 'spin' : ''} />
                {isDeletingDatabase ? '删除中' : '删除数据库'}
              </button>
            </div>
            <div className="danger-item">
              <div>
                <strong>清空媒体文件</strong>
                <span>删除本地图片和视频文件，并重置媒体下载状态。</span>
              </div>
              <button className="btn btn-danger btn-sm" disabled={isClearingMedia} onClick={onClearMedia} type="button">
                <Icon name={isClearingMedia ? 'loader' : 'trash'} size={15} className={isClearingMedia ? 'spin' : ''} />
                {isClearingMedia ? '清空中' : '清空媒体'}
              </button>
            </div>
            <div className="danger-item">
              <div>
                <strong>清空本地缓存</strong>
                <span>清除界面偏好和新手引导状态，不影响 SQLite 与媒体文件。</span>
              </div>
              <button className="btn btn-ghost btn-sm" onClick={clearLocalUiCache} type="button">
                <Icon name="sparkles" size={15} />
                清空缓存
              </button>
            </div>
            <div className="danger-item">
              <div>
                <strong>清理日志</strong>
                <span>删除历史日志文件，保留当前会话日志用于排查问题。</span>
              </div>
              <button className="btn btn-ghost btn-sm" disabled={isClearingLogs} onClick={() => void clearLogs()} type="button">
                <Icon name={isClearingLogs ? 'loader' : 'trash'} size={15} className={isClearingLogs ? 'spin' : ''} />
                {isClearingLogs ? '清理中' : '清理日志'}
              </button>
            </div>
            <div className="danger-item danger-item-primary">
              <div>
                <strong>清空本地库</strong>
                <span>清空 SQLite 收藏、同步记录和媒体目录。该操作不可撤销。</span>
              </div>
              <button className="btn btn-danger btn-sm" disabled={isResetting} onClick={onReset} type="button">
                <Icon name={isResetting ? 'loader' : 'trash'} size={15} className={isResetting ? 'spin' : ''} />
                {isResetting ? '清空中' : '整库清空'}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Cover({
  note,
  overview,
  type,
  category,
  showType = true,
  showPlay = false,
  showMissing = false,
  durationMs,
  glyphSize = 60,
}: {
  note?: NoteSummary;
  overview?: LibraryOverview | null;
  type?: NoteType;
  category?: string | null;
  showType?: boolean;
  showPlay?: boolean;
  showMissing?: boolean;
  durationMs?: number | null;
  glyphSize?: number;
}) {
  const cat = category || note?.categoryName || '未分类';
  const noteType = type || note?.noteType || 'image';
  const seed = hashSeed(note?.id || cat);
  const flags = note ? noteFlags(note) : { remoteMissing: false, coverMissing: false };
  const preview = note ? notePreviewSource(note, overview ?? null) : null;
  const missing = showMissing && !preview?.src && (flags.coverMissing || flags.remoteMissing);
  return (
    <div className="cover" style={{ '--cover-grad': coverGrad(cat, seed) } as CSSProperties}>
      {preview?.kind === 'video' ? (
        <video className="cover-media" muted playsInline preload="none" src={preview.src} />
      ) : preview?.src ? (
        <img alt="" className="cover-media" loading="lazy" referrerPolicy="no-referrer" src={preview.src} />
      ) : (
        <Icon className="cover-glyph" name={NOTE_TYPE[noteType]?.icon ?? 'image'} size={glyphSize} stroke={1.4} />
      )}
      {showType && (
        <span className="cover-type">
          <Icon name={NOTE_TYPE[noteType]?.icon ?? 'image'} size={12} stroke={2.2} />
          {NOTE_TYPE[noteType]?.label}
        </span>
      )}
      {showPlay && noteType === 'video' && (
        <span className="cover-play">
          <Icon name="playLg" size={Math.min(48, glyphSize)} />
        </span>
      )}
      {durationMs ? <span className="cover-dur">{durationFmt(durationMs)}</span> : null}
      {missing && (
        <div className="cover-missing">
          <Icon name={flags.remoteMissing ? 'cloudOff' : 'image'} size={22} />
          <span>{flags.remoteMissing ? '远端已删除' : '封面未下载'}</span>
        </div>
      )}
    </div>
  );
}

function notePreviewSource(note: NoteSummary, overview: LibraryOverview | null) {
  const localCover = note.media.find(
    (asset) =>
      (asset.mediaType === 'cover' || asset.mediaType === 'image') &&
      asset.downloadStatus === 'downloaded' &&
      asset.relativePath,
  );
  const localVideo = note.media.find(
    (asset) => asset.mediaType === 'video' && asset.downloadStatus === 'downloaded' && asset.relativePath,
  );
  const localAsset = localCover ?? localVideo;
  if (localAsset) {
    const localSrc = mediaPreviewSrc(localAsset, overview);
    if (localSrc) {
      return { kind: localAsset.mediaType === 'video' ? 'video' : 'image', src: localSrc };
    }
  }

  const remoteCover =
    note.coverUrl ||
    note.media.find((asset) => (asset.mediaType === 'cover' || asset.mediaType === 'image') && asset.originalUrl)?.originalUrl;
  return remoteCover ? { kind: 'image', src: remoteCover } : null;
}

function notePosterSource(note: NoteSummary, overview: LibraryOverview | null) {
  const localPoster = note.media.find(
    (asset) =>
      (asset.mediaType === 'cover' || asset.mediaType === 'image') &&
      asset.downloadStatus === 'downloaded' &&
      asset.relativePath,
  );
  if (localPoster) {
    const src = mediaPreviewSrc(localPoster, overview);
    if (src) return src;
  }

  return (
    note.coverUrl ||
    note.media.find((asset) => (asset.mediaType === 'cover' || asset.mediaType === 'image') && asset.originalUrl)?.originalUrl ||
    null
  );
}

function Avatar({ name, size = 22 }: { name: string; size?: number }) {
  const palettes: Array<[string, string]> = [
    ['#f6a6c0', '#e2588a'],
    ['#9ba6e8', '#5b63c4'],
    ['#8fcf9c', '#2f9e57'],
    ['#f6b96b', '#e2792f'],
    ['#d7a8e0', '#a85ec0'],
    ['#7fd1c4', '#2f9e8d'],
  ];
  const [a, b] = palettes[((name || '').charCodeAt(0) || 0) % palettes.length];
  return (
    <span
      className="avatar-xs"
      style={{
        width: size,
        height: size,
        background: `linear-gradient(140deg, ${a}, ${b})`,
        fontSize: size * 0.5,
      }}
    >
      {(name || '?').trim().slice(0, 1)}
    </span>
  );
}

function StatusPill({ status, size = 12 }: { status: NoteStatus; size?: number }) {
  const meta = STATUS[status];
  return (
    <span className={`pill ${status}`}>
      <Icon name={meta.icon} size={size} stroke={2.4} />
      {meta.label}
    </span>
  );
}

function EmptyState({ icon = 'inbox', title, desc, actions }: { icon?: IconName; title: string; desc?: string; actions?: ReactNode }) {
  return (
    <div className="empty fade-in">
      <div className="empty-art">
        <Icon name={icon} size={32} stroke={1.7} />
      </div>
      <h3>{title}</h3>
      {desc && <p>{desc}</p>}
      {actions && <div className="empty-actions">{actions}</div>}
    </div>
  );
}

function DownloadBadge({ status }: { status: DownloadStatus }) {
  const meta = DL[status] || DL.not_downloaded;
  return (
    <span className={`dl-badge ${meta.cls}`}>
      <Icon name={meta.icon} size={13} stroke={2.3} className={status === 'downloading' ? 'spin' : ''} />
      {meta.label}
    </span>
  );
}
