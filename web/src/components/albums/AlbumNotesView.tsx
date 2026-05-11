import { ArrowUpRight, FolderOpen } from 'lucide-react';
import type { Album } from '../../types/album';
import type { Note } from '../../types/note';

interface AlbumNotesViewProps {
  album: Album | null;
  notes: Note[];
}

export function AlbumNotesView({ album, notes }: AlbumNotesViewProps) {
  if (!album) {
    return (
      <section className="panel-surface album-notes-view album-notes-view--placeholder">
        <p className="panel-header__eyebrow">Album preview</p>
        <h3>挑一个专辑，看看它现在承接了哪些内容</h3>
        <p>这里适合做后续的“编排式整理”，把路线、攻略、清单和灵感放到一个真正会回看的主题里。</p>
      </section>
    );
  }

  return (
    <section className="panel-surface album-notes-view">
      <div className="panel-header">
        <div>
          <p className="panel-header__eyebrow">Album preview</p>
          <h3>{album.name}</h3>
        </div>
      </div>
      <p className="album-notes-view__description">{album.description}</p>
      <div className="album-notes-view__list">
        {notes.map((note) => (
          <article key={note.id} className="album-note-row">
            <div>
              <strong>{note.title}</strong>
              <span>{note.excerpt}</span>
            </div>
            <div className="album-note-row__meta">
              <span className={`status-pill status-pill--${note.staleness}`}>{note.staleness}</span>
              <ArrowUpRight size={14} />
            </div>
          </article>
        ))}
        {!notes.length && (
          <div className="empty-state empty-state--compact">
            <div className="empty-state__icon">
              <FolderOpen size={18} />
            </div>
            <p>这个专辑现在还是空的，适合先从 `Notes` 批量加入几条相关内容。</p>
          </div>
        )}
      </div>
    </section>
  );
}
