import { ExternalLink, FolderPlus, NotebookText, Tags } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { Album } from '../../types/album';
import type { Note } from '../../types/note';

interface NoteDetailPanelProps {
  note: Note | null;
  albums: Album[];
  saving: boolean;
  onSaveNote: (patch: { userNote?: string; userLabels?: string[]; albumIds?: string[]; archived?: boolean }) => void;
  onAddLabel: (label: string) => void;
  onToggleAlbum: (albumId: string) => void;
  onToggleArchived: () => void;
}

export function NoteDetailPanel({ note, albums, saving, onSaveNote, onAddLabel, onToggleAlbum, onToggleArchived }: NoteDetailPanelProps) {
  const [draftNote, setDraftNote] = useState('');
  const [nextLabel, setNextLabel] = useState('');

  useEffect(() => {
    setDraftNote(note?.userNote ?? '');
    setNextLabel('');
  }, [note?.id, note?.userNote]);

  if (!note) {
    return (
      <aside className="note-detail panel-surface note-detail--placeholder">
        <p className="panel-header__eyebrow">Detail drawer</p>
        <h3>从右侧保留上下文，不用来回跳页</h3>
        <p>点开一条笔记后，这里会承接它的备注、标签、专辑归属和原文跳转。整理动作尽量都在这里完成。</p>
      </aside>
    );
  }

  const allLabels = [...note.systemLabels, ...note.userLabels];

  return (
    <aside className="note-detail panel-surface">
      <div className="note-detail__head">
        <div>
          <p className="panel-header__eyebrow">Detail drawer</p>
          <h3>{note.title}</h3>
        </div>
        <span className={`status-pill status-pill--${note.staleness}`}>{note.staleness}</span>
      </div>

      <p className="note-detail__excerpt">{note.content}</p>

      <div className="note-detail__block">
        <div className="section-label">
          <Tags size={14} />
          标签
        </div>
        <div className="token-cloud token-cloud--dense">
          {allLabels.map((label) => (
            <span
              key={label}
              className={`token-cloud__item token-cloud__item--static${note.userLabels.includes(label) ? ' is-user' : ' is-system'}`}
            >
              {label}
            </span>
          ))}
        </div>
        <div className="inline-form">
          <input
            className="input-field"
            type="text"
            value={nextLabel}
            placeholder="添加一个更贴近自己的标签"
            onChange={(event) => setNextLabel(event.target.value)}
          />
          <button
            type="button"
            className="secondary-button"
            onClick={() => {
              if (!nextLabel.trim()) {
                return;
              }
              onAddLabel(nextLabel.trim());
              setNextLabel('');
            }}
          >
            添加
          </button>
        </div>
      </div>

      <div className="note-detail__block">
        <div className="section-label">
          <FolderPlus size={14} />
          所属专辑
        </div>
        <div className="token-cloud token-cloud--dense">
          {albums.map((album) => (
            <button
              key={album.id}
              type="button"
              className={`token-cloud__item${note.albumIds.includes(album.id) ? ' is-selected' : ''}`}
              onClick={() => onToggleAlbum(album.id)}
            >
              {album.name}
            </button>
          ))}
        </div>
      </div>

      <div className="note-detail__block">
        <div className="section-label">
          <NotebookText size={14} />
          用户备注
        </div>
        <textarea
          className="textarea-field"
          value={draftNote}
          rows={6}
          onChange={(event) => setDraftNote(event.target.value)}
          placeholder="记录为什么留下它、下次回看要先看什么。"
        />
        <div className="note-detail__actions">
          <button
            type="button"
            className="primary-button"
            disabled={saving}
            onClick={() => onSaveNote({ userNote: draftNote })}
          >
            {saving ? '保存中…' : '保存备注'}
          </button>
          <button type="button" className="secondary-button" onClick={onToggleArchived}>
            {note.archived ? '恢复到整理流' : '归档这条笔记'}
          </button>
        </div>
      </div>

      <div className="note-detail__meta-grid">
        <div>
          <span>发布时间</span>
          <strong>{note.publishDate}</strong>
        </div>
        <div>
          <span>最近同步</span>
          <strong>{note.fetchedAt}</strong>
        </div>
      </div>

      <a className="detail-link" href={note.sourceUrl} target="_blank" rel="noreferrer">
        查看原文
        <ExternalLink size={14} />
      </a>
    </aside>
  );
}
