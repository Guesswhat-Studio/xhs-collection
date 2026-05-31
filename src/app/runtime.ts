import { openPath, openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';

export function isTauriRuntime() {
  if (typeof window === 'undefined') return false;
  const tauri = (window as unknown as { __TAURI_INTERNALS__?: { transformCallback?: unknown } }).__TAURI_INTERNALS__;
  return typeof tauri?.transformCallback === 'function';
}

export function joinLocalPath(base: string, relative: string) {
  const separator = base.includes('\\') ? '\\' : '/';
  const cleanBase = base.replace(/[\\/]+$/, '');
  const cleanRelative = relative.replace(/^[\\/]+/, '').replace(/[\\/]+/g, separator);
  return `${cleanBase}${separator}${cleanRelative}`;
}

export async function openLocalPath(path: string | null, reveal = false) {
  if (!path) return;
  if (reveal) {
    await revealItemInDir(path);
  } else {
    await openPath(path);
  }
}

export async function openExternalUrl(url: string) {
  await openUrl(url);
}
