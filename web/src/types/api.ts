export interface ApiError {
  code: string;
  message: string;
  details?: string;
}

export interface ApiEnvelope<T> {
  data: T;
  error: ApiError | null;
  meta?: Record<string, unknown>;
}

export interface StatsBreakdown {
  label: string;
  count: number;
}

export interface StatsSnapshot {
  totalNotes: number;
  activeNotes: number;
  archivedNotes: number;
  albumsCount: number;
  labelCount: number;
  freshCount: number;
  agingCount: number;
  outdatedCount: number;
  lastSyncedAt: string;
  bySystemLabel: StatsBreakdown[];
  byAccount: StatsBreakdown[];
}
