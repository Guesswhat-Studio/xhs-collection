import { Archive, CircleDashed, Filter, Sparkles, Tags } from 'lucide-react';
import type { LabelStat } from '../../types/label';
import type { NoteFilters as NoteFiltersState } from '../../types/note';

interface NoteFiltersProps {
  accounts: string[];
  labels: LabelStat[];
  suggestedLabels: string[];
  filters: NoteFiltersState;
  onChange: (patch: Partial<NoteFiltersState>) => void;
}

const stalenessOptions = [
  { value: 'all', label: '全部时效' },
  { value: 'fresh', label: 'Fresh' },
  { value: 'aging', label: 'Aging' },
  { value: 'outdated', label: 'Outdated' },
] as const;

export function NoteFilters({ accounts, labels, suggestedLabels, filters, onChange }: NoteFiltersProps) {
  return (
    <aside className="note-filters panel-surface">
      <div className="panel-header">
        <div>
          <p className="panel-header__eyebrow">Filter rail</p>
          <h3>
            <Filter size={16} />
            缩小范围
          </h3>
        </div>
      </div>

      <div className="note-filters__section">
        <label className="field-label" htmlFor="search-field">
          搜索
        </label>
        <input
          id="search-field"
          className="input-field"
          type="search"
          value={filters.searchText}
          placeholder="试试搜东京、探店、清单、标签…"
          onChange={(event) => onChange({ searchText: event.target.value })}
        />
      </div>

      <div className="note-filters__section">
        <div className="section-label">
          <CircleDashed size={14} />
          时效性
        </div>
        <div className="chip-grid">
          {stalenessOptions.map((option) => (
            <button
              key={option.value}
              type="button"
              className={`chip-button${filters.staleness === option.value ? ' is-selected' : ''}`}
              onClick={() => onChange({ staleness: option.value })}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="note-filters__section">
        <div className="section-label">
          <Sparkles size={14} />
          账号
        </div>
        <div className="stacked-options">
          <button
            type="button"
            className={`stacked-option${filters.accountName === 'all' ? ' is-selected' : ''}`}
            onClick={() => onChange({ accountName: 'all' })}
          >
            全部账号
          </button>
          {accounts.map((account) => (
            <button
              key={account}
              type="button"
              className={`stacked-option${filters.accountName === account ? ' is-selected' : ''}`}
              onClick={() => onChange({ accountName: account })}
            >
              {account}
            </button>
          ))}
        </div>
      </div>

      <div className="note-filters__section">
        <div className="section-label">
          <Tags size={14} />
          推荐标签
        </div>
        <div className="chip-grid">
          <button
            type="button"
            className={`chip-button${filters.label === 'all' ? ' is-selected' : ''}`}
            onClick={() => onChange({ label: 'all' })}
          >
            全部标签
          </button>
          {suggestedLabels.map((label) => (
            <button
              key={label}
              type="button"
              className={`chip-button${filters.label === label ? ' is-selected' : ''}`}
              onClick={() => onChange({ label })}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="note-filters__section">
        <div className="section-label">所有标签</div>
        <div className="token-cloud">
          {labels.slice(0, 14).map((label) => (
            <button
              key={label.name}
              type="button"
              className={`token-cloud__item${filters.label === label.name ? ' is-selected' : ''}`}
              onClick={() => onChange({ label: label.name })}
            >
              {label.name}
              <span>{label.count}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="note-filters__section note-filters__section--toggle">
        <button
          type="button"
          className={`toggle-chip${filters.showArchived ? ' is-selected' : ''}`}
          onClick={() => onChange({ showArchived: !filters.showArchived })}
        >
          <Archive size={14} />
          {filters.showArchived ? '正在包含归档内容' : '默认隐藏归档内容'}
        </button>
      </div>
    </aside>
  );
}
