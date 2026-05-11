export type Staleness = 'fresh' | 'aging' | 'outdated';
export type NoteViewMode = 'grid' | 'list';

export interface Note {
  id: string;
  title: string;
  excerpt: string;
  content: string;
  heroLabel: string;
  tone: 'travel' | 'food' | 'beauty' | 'planning';
  accountName: string;
  publishDate: string;
  fetchedAt: string;
  sourceUrl: string;
  rawTags: string[];
  systemLabels: string[];
  userLabels: string[];
  albumIds: string[];
  userNote: string;
  staleness: Staleness;
  archived: boolean;
}

export interface NoteFilters {
  searchText: string;
  staleness: Staleness | 'all';
  accountName: string | 'all';
  label: string | 'all';
  showArchived: boolean;
}

export interface UpdateNoteInput {
  id: string;
  userNote?: string;
  userLabels?: string[];
  albumIds?: string[];
  archived?: boolean;
}

export interface BulkNoteActionInput {
  ids: string[];
  action: 'archive' | 'restore' | 'tag-focus' | 'album-weekend';
}
