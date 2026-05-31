import type { NoteStatus, NoteSummary } from '../types/library';
import { parseAppTime, relTime } from './formatUtils';

export type LibraryFilter = NoteStatus | 'all' | 'attention';

const COVER_PALETTE: Record<string, [string, string]> = {
  旅行: ['#84c7d9', '#4f83c5'],
  美食: ['#f6b96b', '#e2792f'],
  装修: ['#9aa7b8', '#5a6b80'],
  投资: ['#9ba6e8', '#5b63c4'],
  学习: ['#8fcf9c', '#2f9e57'],
  购物: ['#f29bb6', '#e2588a'],
  灵感: ['#d7a8e0', '#a85ec0'],
  未分类: ['#c8bcc0', '#897e84'],
};

export function splitTagInput(value: string) {
  const seen = new Set<string>();
  return value
    .split(/[,，;；\n\t]/)
    .map((item) => item.trim().replace(/^#/, '').trim())
    .filter((item) => {
      if (!item) return false;
      const key = item.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .slice(0, 32);
}

export function matchFilter(note: NoteSummary, filter: LibraryFilter) {
  if (filter === 'all') return true;
  if (filter === 'attention') {
    const flags = noteFlags(note);
    return flags.remoteMissing || flags.failed || flags.coverMissing;
  }
  return note.status === filter;
}

export function noteFlags(note: NoteSummary) {
  const remoteMissing = Boolean(note.remoteMissingAt);
  const failed = note.media.some((asset) => asset.downloadStatus === 'failed');
  const pendingMedia = note.media.filter((asset) => asset.mediaType !== 'video' && asset.downloadStatus !== 'downloaded').length;
  const hasLocalPreview = note.media.some(
    (asset) =>
      (asset.mediaType === 'cover' || asset.mediaType === 'image' || asset.mediaType === 'video') &&
      asset.downloadStatus === 'downloaded',
  );
  const coverMissing = note.media.length > 0 && !hasLocalPreview;
  return { remoteMissing, failed, pendingMedia, coverMissing };
}

export function noteTime(note: NoteSummary) {
  const collected = parseAppTime(note.collectedAt);
  if (!Number.isNaN(collected)) return collected;
  if (note.favoriteOrder) return Date.now() - note.favoriteOrder;
  return parseAppTime(note.lastSeenAt ?? note.lastSyncedAt);
}

export function coverPalette(category?: string | null): [string, string] {
  return COVER_PALETTE[category || '未分类'] || COVER_PALETTE['未分类'];
}

export function coverGrad(category?: string | null, seed = 150) {
  const [a, b] = coverPalette(category);
  return `linear-gradient(${seed}deg, ${a}, ${b})`;
}

export function hashSeed(value?: string | null) {
  let hash = 0;
  for (let index = 0; index < (value || '').length; index += 1) {
    hash = (hash * 31 + (value || '').charCodeAt(index)) % 360;
  }
  return 110 + (hash % 70);
}

export function favTimeLabel(note: NoteSummary) {
  if (note.collectedAt) return `收藏于 ${relTime(note.collectedAt)}`;
  if (note.favoriteOrder) return `收藏序 #${note.favoriteOrder}`;
  return `同步 ${relTime(note.lastSeenAt ?? note.lastSyncedAt)}`;
}
