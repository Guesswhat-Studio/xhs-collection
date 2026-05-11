import { CalendarDays, Check, CircleDot, FolderPlus, NotebookPen } from 'lucide-react';
import type { Note } from '../../types/note';

interface NoteCardProps {
  note: Note;
  isSelected: boolean;
  isActive: boolean;
  onSelectToggle: (id: string) => void;
  onActivate: (id: string) => void;
}

const toneCopy: Record<Note['tone'], string> = {
  travel: '路线感',
  food: '带人去也稳',
  beauty: '回看频率高',
  planning: '适合做专题',
};

export function NoteCard({ note, isSelected, isActive, onSelectToggle, onActivate }: NoteCardProps) {
  return (
    <article
      className={`note-card note-card--${note.tone}${isSelected ? ' is-selected' : ''}${isActive ? ' is-active' : ''}`}
      onClick={() => onActivate(note.id)}
    >
      <div className="note-card__cover">
        <span className="note-card__hero-label">{note.heroLabel}</span>
        <span className={`status-pill status-pill--${note.staleness}`}>{note.staleness}</span>
      </div>

      <div className="note-card__body">
        <div className="note-card__meta-row">
          <span>{note.accountName}</span>
          <button
            type="button"
            className={`selection-toggle${isSelected ? ' is-selected' : ''}`}
            aria-pressed={isSelected}
            aria-label={isSelected ? '取消选择' : '选择这条笔记'}
            onClick={(event) => {
              event.stopPropagation();
              onSelectToggle(note.id);
            }}
          >
            {isSelected ? <Check size={14} /> : <CircleDot size={14} />}
          </button>
        </div>

        <h3>{note.title}</h3>
        <p>{note.excerpt}</p>

        <div className="note-card__labels">
          {note.systemLabels.slice(0, 2).map((label) => (
            <span key={label} className="meta-chip meta-chip--system">
              {label}
            </span>
          ))}
          {note.userLabels.slice(0, 2).map((label) => (
            <span key={label} className="meta-chip meta-chip--user">
              {label}
            </span>
          ))}
        </div>

        <div className="note-card__footer">
          <span>
            <CalendarDays size={14} />
            {note.publishDate}
          </span>
          <span>
            <FolderPlus size={14} />
            {note.albumIds.length || '未分配'}
          </span>
          <span>
            <NotebookPen size={14} />
            {toneCopy[note.tone]}
          </span>
        </div>
      </div>
    </article>
  );
}
