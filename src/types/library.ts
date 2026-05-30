export type NoteStatus = 'unread' | 'read' | 'outdated' | 'archived';
export type NoteType = 'video' | 'image' | 'article' | 'unknown';
export type DownloadStatus = 'not_downloaded' | 'queued' | 'downloading' | 'downloaded' | 'failed';

export interface MediaAsset {
  id: string;
  noteId: string;
  mediaType: 'video' | 'image' | 'cover';
  downloadStatus: DownloadStatus;
  originalUrl?: string | null;
  relativePath?: string | null;
  mimeType?: string | null;
  sizeBytes?: number | null;
  width?: number | null;
  height?: number | null;
  durationMs?: number | null;
}

export interface NoteSummary {
  id: string;
  source: string;
  sourceNoteId: string;
  sourceUrl: string;
  title: string;
  excerpt: string;
  content: string;
  authorName: string;
  coverUrl?: string | null;
  noteType: NoteType;
  publishedAt?: string | null;
  collectedAt?: string | null;
  favoriteOrder?: number | null;
  lastSyncedAt: string;
  lastSeenAt?: string | null;
  remoteMissingAt?: string | null;
  remoteStatus?: string;
  unavailableReason?: string | null;
  status: NoteStatus;
  categoryName?: string | null;
  userNote: string;
  tags: string[];
  media: MediaAsset[];
}

export interface LibraryOverview {
  appDataDir: string;
  dbPath: string;
  mediaDir: string;
  notesCount: number;
  mediaCount: number;
  storageRootId: string;
  activeProfile: LocalProfileSummary;
  profiles: LocalProfileSummary[];
}

export interface LocalProfileSummary {
  id: string;
  displayName: string;
  source?: string | null;
  sourceAccountId?: string | null;
  avatarUrl?: string | null;
  dbPath: string;
  mediaDir: string;
  isActive: boolean;
  sessionStatus: string;
  lastOpenedAt?: string | null;
  lastSyncAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface StatusUpdateInput {
  noteId: string;
  status: NoteStatus;
}

export interface NoteMetadataUpdateInput {
  noteId: string;
  status?: NoteStatus | null;
  categoryName?: string | null;
  tags?: string[] | null;
  userNote?: string | null;
}

export interface BatchNoteMetadataUpdateInput {
  noteIds: string[];
  status?: NoteStatus | null;
  categoryName?: string | null;
  addTags?: string[] | null;
}

export interface AiSettings {
  provider: 'openai_compatible' | 'claude';
  baseUrl: string;
  model: string;
  hasApiKey: boolean;
  temperature: number;
  maxTokens: number;
  updatedAt?: string | null;
}

export interface AiSettingsInput {
  provider: 'openai_compatible' | 'claude';
  baseUrl: string;
  model: string;
  apiKey?: string | null;
  clearApiKey?: boolean | null;
  temperature?: number | null;
  maxTokens?: number | null;
}

export interface AiSettingsTestResult {
  ok: boolean;
  message: string;
  model: string;
}

export interface AiClassifyInput {
  limit?: number | null;
}

export interface AiSplitCategoryInput {
  sourceCategoryName: string;
  targetCategoryName: string;
  query: string;
  limit?: number | null;
}

export interface AiTagGroupInput {
  limit?: number | null;
}

export interface AiAssignmentResult {
  noteId: string;
  title: string;
  categoryName: string;
  confidence: number;
  reason: string;
}

export interface AiClassificationResult {
  scanned: number;
  updated: number;
  createdCategories: string[];
  assignments: AiAssignmentResult[];
  message: string;
}

export interface TagSummary {
  name: string;
  count: number;
  kind: string;
  groupName?: string | null;
}

export interface AiTagAssignmentResult {
  tag: string;
  groupName: string;
  confidence: number;
  reason: string;
}

export interface AiTagGroupResult {
  scanned: number;
  updated: number;
  groups: string[];
  assignments: AiTagAssignmentResult[];
  message: string;
}

export interface XhsSessionTestResult {
  ok: boolean;
  statusCode: number;
  finalUrl: string;
  pageTitle?: string | null;
  accountHint?: string | null;
  accountId?: string | null;
  accountName?: string | null;
  avatarUrl?: string | null;
  cookieKeys: string[];
  checkedAt: string;
  message: string;
}

export interface XhsFavoriteSyncInput {
  maxCount?: number | null;
  resume?: boolean;
  fullSync?: boolean;
}

export interface XhsFavoriteSyncResult {
  scanned: number;
  fetched: number;
  inserted: number;
  updated: number;
  skipped: number;
  existingSkipped: number;
  remoteMissing: number;
  remoteDisplayCount?: number | null;
  remoteUnreturnedCount?: number | null;
  limitReached?: boolean;
  fullSync?: boolean;
  detailsUpdated?: number;
  detailsFailed?: number;
  coversDownloaded?: number;
  coversFailed?: number;
  mediaDownloaded?: number;
  mediaFailed?: number;
  message: string;
}

export interface XhsSyncProgress {
  phase: string;
  label: string;
  detail: string;
  planned?: number | null;
  scanned?: number;
  fetched: number;
  toSync?: number | null;
  written: number;
  inserted: number;
  updated: number;
  skipped: number;
  existingSkipped?: number;
  progress: number;
  indeterminate: boolean;
}

export interface BatchJobInput {
  limit?: number | null;
  assetId?: string | null;
  noteId?: string | null;
}

export interface BatchJobResult {
  scanned: number;
  updated: number;
  downloaded: number;
  failed: number;
  skipped: number;
  message: string;
}

export interface BatchJobProgress {
  phase: string;
  label: string;
  detail: string;
  planned: number;
  scanned: number;
  updated: number;
  downloaded: number;
  failed: number;
  skipped: number;
  progress: number;
  indeterminate: boolean;
}

export interface ExportLibraryInput {
  format: 'json' | 'csv' | 'markdown';
  includeMedia?: boolean | null;
  includeNotes?: boolean | null;
  onlyReviewed?: boolean | null;
}

export interface ExportLibraryResult {
  path: string;
  format: string;
  noteCount: number;
  mediaCount: number;
  message: string;
}

export interface LibraryApi {
  getLibraryOverview(): Promise<LibraryOverview>;
  listLocalProfiles(): Promise<LocalProfileSummary[]>;
  switchLocalProfile(profileId: string): Promise<LibraryOverview>;
  deleteLibraryDatabase(): Promise<LibraryOverview>;
  clearMediaFiles(): Promise<LibraryOverview>;
  resetLibraryData(): Promise<LibraryOverview>;
  listNotes(): Promise<NoteSummary[]>;
  updateNoteStatus(input: StatusUpdateInput): Promise<NoteSummary[]>;
  updateNoteMetadata(input: NoteMetadataUpdateInput): Promise<NoteSummary[]>;
  batchUpdateNoteMetadata(input: BatchNoteMetadataUpdateInput): Promise<NoteSummary[]>;
  loadAiSettings(): Promise<AiSettings>;
  saveAiSettings(input: AiSettingsInput): Promise<AiSettings>;
  testAiSettings(): Promise<AiSettingsTestResult>;
  listTags(): Promise<TagSummary[]>;
  aiClassifyUncategorized(input: AiClassifyInput): Promise<AiClassificationResult>;
  aiSplitCategory(input: AiSplitCategoryInput): Promise<AiClassificationResult>;
  aiGroupTags(input: AiTagGroupInput): Promise<AiTagGroupResult>;
  exportLibrary(input: ExportLibraryInput): Promise<ExportLibraryResult>;
  loadXhsSavedSession(): Promise<XhsSessionTestResult | null>;
  openXhsLoginWindow(): Promise<void>;
  readXhsLoginCookies(): Promise<XhsSessionTestResult>;
  testXhsSession(cookie: string): Promise<XhsSessionTestResult>;
  syncXhsFavorites(input: XhsFavoriteSyncInput): Promise<XhsFavoriteSyncResult>;
  enrichXhsNoteDetails(input: BatchJobInput): Promise<BatchJobResult>;
  downloadMediaAssets(input: BatchJobInput): Promise<BatchJobResult>;
}
