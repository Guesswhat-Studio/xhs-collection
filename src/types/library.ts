export type NoteStatus = 'unread' | 'read' | 'outdated' | 'archived';
export type NoteType = 'video' | 'image' | 'article' | 'unknown';
export type DownloadStatus = 'not_downloaded' | 'queued' | 'downloading' | 'downloaded' | 'failed';

export interface MediaAsset {
  id: string;
  noteId: string;
  mediaType: 'video' | 'image' | 'cover' | 'file';
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
  contentCoverage: LibraryContentCoverage;
  storageRootId: string;
  activeProfile: LocalProfileSummary;
  profiles: LocalProfileSummary[];
}

export interface LibraryContentCoverage {
  totalNotes: number;
  detailNotes: number;
  taggedNotes: number;
  mediaNotes: number;
  missingDetailNotes: number;
  missingTagNotes: number;
  uniqueTags: number;
}

export interface LogFileInfo {
  logDir: string;
  currentLogPath: string;
  latestLogPath: string;
  currentLogName: string;
  latestLogName: string;
  currentLogExists: boolean;
  latestLogExists: boolean;
}

export interface ClearLogFilesResult {
  deleted: number;
  kept: number;
  failed: string[];
  message: string;
  info: LogFileInfo;
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

export interface AiPromptEditorItem {
  key: string;
  label: string;
  system: string;
  user?: string | null;
  task?: string | null;
  rules: string[];
  schemaKind?: 'output_schema' | 'return_json_shape' | string | null;
  schemaText?: string | null;
}

export interface AiPromptSettings {
  path: string;
  isCustom: boolean;
  validationError?: string | null;
  prompts: AiPromptEditorItem[];
}

export interface AiPromptSettingsInput {
  prompts: AiPromptEditorItem[];
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

export interface AiTagMergeSuggestInput {
  limit?: number | null;
  useAi?: boolean | null;
  minConfidence?: number | null;
}

export interface TagMergeGroupInput {
  canonicalTag: string;
  duplicateTags: string[];
  confidence?: number | null;
  source?: string | null;
}

export interface TagGovernanceApplyInput {
  removeTags: string[];
  mergeGroups: TagMergeGroupInput[];
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

export interface TagGroupClearResult {
  scanned: number;
  cleared: number;
  message: string;
}

export interface TagCleanupIssue {
  tag: string;
  count: number;
  issueKind: string;
  action: string;
  confidence: number;
  reason: string;
}

export interface TagMergeSuggestion {
  canonicalTag: string;
  duplicateTags: string[];
  affectedNotes: number;
  confidence: number;
  reason: string;
  source: 'rule' | 'ai' | string;
}

export interface TagGovernanceSuggestionResult {
  scanned: number;
  cleanupIssues: TagCleanupIssue[];
  mergeGroups: TagMergeSuggestion[];
  message: string;
}

export interface TagGovernanceApplyResult {
  removedTags: number;
  mergedTags: number;
  aliasesCreated: number;
  affectedNotes: number;
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

export interface XhsAlbumSyncInput {
  maxAlbums?: number | null;
  maxNotesPerAlbum?: number | null;
}

export interface XhsAlbumSyncResult {
  albumsScanned: number;
  albumsUpdated: number;
  notesScanned: number;
  notesLinked: number;
  notesInserted: number;
  notesUpdated: number;
  duplicateNotes: number;
  skipped: number;
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

export interface AiJobProgress {
  task: 'classify_uncategorized' | 'split_category' | 'group_tags' | string;
  phase: string;
  label: string;
  detail: string;
  planned: number;
  scanned: number;
  updated: number;
  failed: number;
  skipped: number;
  progress: number;
  indeterminate: boolean;
  error?: string | null;
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

export interface LibraryBackupResult {
  path: string;
  fileCount: number;
  sizeBytes: number;
  message: string;
}

export interface LibraryApi {
  getLibraryOverview(): Promise<LibraryOverview>;
  getLogFileInfo(): Promise<LogFileInfo>;
  clearLogFiles(): Promise<ClearLogFilesResult>;
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
  loadAiPromptSettings(): Promise<AiPromptSettings>;
  saveAiPromptSettings(input: AiPromptSettingsInput): Promise<AiPromptSettings>;
  resetAiPromptSettings(): Promise<AiPromptSettings>;
  listTags(): Promise<TagSummary[]>;
  clearAiTagGroups(): Promise<TagGroupClearResult>;
  cancelAiTask(task?: string): Promise<void>;
  aiClassifyUncategorized(input: AiClassifyInput): Promise<AiClassificationResult>;
  aiSplitCategory(input: AiSplitCategoryInput): Promise<AiClassificationResult>;
  aiGroupTags(input: AiTagGroupInput): Promise<AiTagGroupResult>;
  aiSuggestTagMerges(input: AiTagMergeSuggestInput): Promise<TagGovernanceSuggestionResult>;
  applyTagGovernance(input: TagGovernanceApplyInput): Promise<TagGovernanceApplyResult>;
  exportLibrary(input: ExportLibraryInput): Promise<ExportLibraryResult>;
  createLibraryBackup(): Promise<LibraryBackupResult>;
  loadXhsSavedSession(): Promise<XhsSessionTestResult | null>;
  openXhsLoginWindow(): Promise<void>;
  readXhsLoginCookies(): Promise<XhsSessionTestResult>;
  testXhsSession(cookie: string): Promise<XhsSessionTestResult>;
  cancelXhsSync(): Promise<void>;
  syncXhsFavorites(input: XhsFavoriteSyncInput): Promise<XhsFavoriteSyncResult>;
  syncXhsFiles(input: XhsFavoriteSyncInput): Promise<XhsFavoriteSyncResult>;
  syncXhsAlbums(input: XhsAlbumSyncInput): Promise<XhsAlbumSyncResult>;
  enrichXhsNoteDetails(input: BatchJobInput): Promise<BatchJobResult>;
  downloadMediaAssets(input: BatchJobInput): Promise<BatchJobResult>;
}
