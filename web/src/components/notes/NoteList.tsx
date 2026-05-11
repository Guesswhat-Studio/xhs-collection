import { ChevronRight, NotebookTabs } from 'lucide-react';
import type { Note, NoteViewMode } from '../../types/note';
import { NoteCard } from './NoteCard';

interface NoteListProps {
  notes: Note[];
  viewMode: NoteViewMode;
  activeNoteId: string | null;
  selectedIds: string[];
  onActivate: (id: string) => void;
  onSelectToggle: (id: string) => void;
}

function NoteListRow({
  note,
  isActive,
  isSelected,
  onActivate,
  onSelectToggle,
}: {
  note: Note;
  isActive: boolean;
  isSelected: boolean;
  onActivate: (id: string) => void;
  onSelectToggle: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className={`note-list-row${isActive ? ' is-active' : ''}${isSelected ? ' is-selected' : ''}`}
      onClick={() => onActivate(note.id)}
    >
      <span className="note-list-row__badge">{note.heroLabel}</span>
      <div className="note-list-row__content">
        <strong>{note.title}</strong>
        <span>{note.excerpt}</span>
      </div>
      <div className="note-list-row__meta">
        <span className={`status-pill status-pill--${note.staleness}`}>{note.staleness}</span>
        <span>{note.accountName}</span>
        <button
          type="button"
          className={`selection-toggle${isSelected ? ' is-selected' : ''}`}
          aria-pressed={isSelected}
          onClick={(event) => {
            event.stopPropagation();
            onSelectToggle(note.id);
          }}
        >
          {isSelected ? '已选' : '选择'}
        </button>
        <ChevronRight size={16} />
      </div>
    </button>
  );
}

export function NoteList({ note, notes, viewMode, activeNoteId, selectedIds, onActivate, onSelectToggle }: NoteListProps & { note?: never }) {
  if (!notes.length) {
    return (
      <div className="empty-state panel-surface">
        <div className="empty-state__icon">
          <NotebookTabs size={20} />
        </div>
        <h3>这一屏先空下来也没关系</h3>
        <p>你现在的筛选条件已经把范围缩得很小了。试试清掉一个标签、换一个关键词，或者切回默认视角。</p>
      </div>
    );
  }

  if (viewMode === 'list') {
    return (
      <div className="note-list note-list--rows">
        {notes.map((item) => (
          <NoteListRow
            key={item.id}
            note={item}
            isActive={activeNoteId === item.id}
            isSelected={selectedIds.includes(item.id)}
            onActivate={onActivate}
            onSelectToggle={onSelectToggle}
          />
        ))}
      </div>
    );
  }

  return (
    <div className="note-list note-list--grid">
      {notes.map((item) => (
        <NoteCard
          key={item.id}
          note={item}
          isActive={activeNoteId === item.id}
          isSelected={selectedIds.includes(item.id)}
          onActivate={onActivate}
          onSelectToggle={onSelectToggle}
        />
      ))}
    </div>
  );
}
