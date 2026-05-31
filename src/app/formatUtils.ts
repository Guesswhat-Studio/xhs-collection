export function relTime(value?: string | null) {
  if (!value) return '';
  const time = parseAppTime(value);
  if (Number.isNaN(time)) return '';
  const diff = Date.now() - time;
  const minute = Math.floor(diff / 60000);
  if (minute < 1) return '刚刚';
  if (minute < 60) return `${minute} 分钟前`;
  const hour = Math.floor(minute / 60);
  if (hour < 24) return `${hour} 小时前`;
  const day = Math.floor(hour / 24);
  if (day === 1) return '昨天';
  if (day < 7) return `${day} 天前`;
  if (day < 30) return `${Math.floor(day / 7)} 周前`;
  if (day < 365) return `${Math.floor(day / 30)} 个月前`;
  return `${Math.floor(day / 365)} 年前`;
}

export function shortDate(value?: string | null) {
  if (!value) return '';
  const date = appDate(value);
  if (Number.isNaN(date.getTime())) return value;
  return `${date.getFullYear()}.${String(date.getMonth() + 1).padStart(2, '0')}.${String(date.getDate()).padStart(2, '0')}`;
}

export function fullDate(value?: string | null) {
  if (!value) return '-';
  const date = appDate(value);
  if (Number.isNaN(date.getTime())) return value;
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

export function appDate(value?: string | null) {
  return new Date(normalizeAppTimestamp(value));
}

export function parseAppTime(value?: string | null) {
  return appDate(value).getTime();
}

export function normalizeAppTimestamp(value?: string | null) {
  const text = (value ?? '').trim();
  if (!text) return 'Invalid Date';
  if (/^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?$/.test(text)) {
    return `${text.replace(' ', 'T')}Z`;
  }
  return text;
}

export function fileSize(bytes?: number | null) {
  if (!bytes) return '-';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

export function coveragePercent(value: number, total: number) {
  if (!total) return '0%';
  return `${Math.round((value / total) * 100)}%`;
}

export function durationFmt(ms?: number | null) {
  if (!ms) return '';
  const seconds = Math.round(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
