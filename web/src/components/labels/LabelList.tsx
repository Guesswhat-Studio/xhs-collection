import { Sparkles, Tags } from 'lucide-react';
import type { LabelStat } from '../../types/label';

interface LabelListProps {
  labels: LabelStat[];
  activeLabel: string | null;
  onSelect: (label: string | null) => void;
}

export function LabelList({ labels, activeLabel, onSelect }: LabelListProps) {
  return (
    <section className="panel-surface label-list-panel">
      <div className="panel-header">
        <div>
          <p className="panel-header__eyebrow">Label cloud</p>
          <h3>
            <Tags size={16} />
            常用标签
          </h3>
        </div>
      </div>

      <div className="token-cloud">
        <button
          type="button"
          className={`token-cloud__item${activeLabel === null ? ' is-selected' : ''}`}
          onClick={() => onSelect(null)}
        >
          全部标签
        </button>
        {labels.map((label) => (
          <button
            key={label.name}
            type="button"
            className={`token-cloud__item${activeLabel === label.name ? ' is-selected' : ''}`}
            onClick={() => onSelect(label.name)}
          >
            {label.name}
            <span>{label.count}</span>
          </button>
        ))}
      </div>

      <div className="label-list-panel__hint">
        <Sparkles size={14} />
        系统标签负责兜底分类，用户标签才决定你以后会怎么回看。
      </div>
    </section>
  );
}
