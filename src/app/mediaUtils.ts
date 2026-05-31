import { convertFileSrc } from '@tauri-apps/api/core';
import type { CSSProperties } from 'react';
import type { LibraryOverview, MediaAsset } from '../types/library';
import { isTauriRuntime, joinLocalPath } from './runtime';

export function mediaAbsolutePath(asset: MediaAsset, overview: LibraryOverview | null) {
  if (!overview?.mediaDir || !asset.relativePath) return null;
  return joinLocalPath(overview.mediaDir, asset.relativePath);
}

export function mediaPreviewSrc(asset: MediaAsset, overview: LibraryOverview | null) {
  if (asset.mediaType === 'file') return null;
  const absolutePath = mediaAbsolutePath(asset, overview);
  if (!absolutePath || !isTauriRuntime()) return null;
  return convertFileSrc(absolutePath);
}

export function mediaAspectRatio(asset: MediaAsset) {
  const width = Number(asset.width ?? 0);
  const height = Number(asset.height ?? 0);
  if (width > 0 && height > 0) return width / height;
  if (asset.mediaType === 'video') return 9 / 16;
  if (asset.mediaType === 'cover') return 4 / 3;
  if (asset.mediaType === 'file') return 4 / 3;
  return 1;
}

export function mediaAspectStyle(asset: MediaAsset) {
  const ratio = mediaAspectRatio(asset);
  return {
    '--media-ratio': ratio,
    aspectRatio: `${ratio}`,
  } as CSSProperties;
}

export function mediaAspectLabel(asset: MediaAsset) {
  if (asset.mediaType === 'file') return '文件';
  const width = Number(asset.width ?? 0);
  const height = Number(asset.height ?? 0);
  const ratio = mediaAspectRatio(asset);
  const direction = ratio < 0.85 ? '竖屏' : ratio > 1.25 ? '横屏' : '方图';
  return width > 0 && height > 0 ? `${width} x ${height} · ${direction}` : `${direction}预览`;
}
