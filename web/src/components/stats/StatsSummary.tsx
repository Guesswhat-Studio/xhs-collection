import { Archive, FolderKanban, NotebookTabs, Tags } from 'lucide-react';
import type { StatsSnapshot } from '../../types/api';

interface StatsSummaryProps {
  stats: StatsSnapshot;
}

const cards = [
  { key: 'activeNotes', label: '正在整理流中的笔记', icon: NotebookTabs },
  { key: 'albumsCount', label: '已经形成的专辑', icon: FolderKanban },
  { key: 'labelCount', label: '标签总数', icon: Tags },
  { key: 'archivedNotes', label: '已归档内容', icon: Archive },
] as const;

export function StatsSummary({ stats }: StatsSummaryProps) {
  return (
    <section className="stats-summary">
      {cards.map(({ key, label, icon: Icon }) => (
        <article key={key} className="panel-surface stats-summary__card">
          <div className="stats-summary__icon">
            <Icon size={18} />
          </div>
          <strong>{stats[key]}</strong>
          <span>{label}</span>
        </article>
      ))}
    </section>
  );
}
