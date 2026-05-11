export interface LabelStat {
  name: string;
  count: number;
  kind: 'system' | 'user';
}
