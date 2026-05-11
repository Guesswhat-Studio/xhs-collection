import type { LabelStat } from '../../types/label';
import type { Note } from '../../types/note';

interface LabelUsageTableProps {
  labels: LabelStat[];
  notes: Note[];
}

export function LabelUsageTable({ labels, notes }: LabelUsageTableProps) {
  return (
    <section className="panel-surface label-usage-table">
      <div className="panel-header">
        <div>
          <p className="panel-header__eyebrow">Usage table</p>
          <h3>标签出现在哪些回看情境里</h3>
        </div>
      </div>

      <div className="table-shell">
        <div className="table-shell__head">
          <span>标签</span>
          <span>类型</span>
          <span>出现次数</span>
          <span>示例笔记</span>
        </div>
        {labels.map((label) => {
          const related = notes.filter((note) => [...note.systemLabels, ...note.userLabels].includes(label.name)).slice(0, 2);

          return (
            <div key={label.name} className="table-shell__row">
              <strong>{label.name}</strong>
              <span>{label.kind === 'system' ? 'system' : 'user'}</span>
              <span>{label.count}</span>
              <span>{related.map((note) => note.title).join(' · ') || '—'}</span>
            </div>
          );
        })}
      </div>
    </section>
  );
}
