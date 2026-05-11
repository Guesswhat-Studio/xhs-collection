import { CalendarDays, FolderHeart } from 'lucide-react';
import type { Album } from '../../types/album';

interface AlbumListProps {
  albums: Album[];
  activeAlbumId: string | null;
  onSelect: (id: string) => void;
}

export function AlbumList({ albums, activeAlbumId, onSelect }: AlbumListProps) {
  return (
    <div className="album-list">
      {albums.map((album) => (
        <button
          key={album.id}
          type="button"
          className={`album-card album-card--${album.tone}${activeAlbumId === album.id ? ' is-active' : ''}`}
          onClick={() => onSelect(album.id)}
        >
          <div className="album-card__cover">
            <span>{album.tone}</span>
            <strong>{album.noteIds.length} 条</strong>
          </div>
          <div className="album-card__body">
            <h3>
              <FolderHeart size={16} />
              {album.name}
            </h3>
            <p>{album.description}</p>
            <div className="album-card__meta">
              <span>
                <CalendarDays size={14} />
                更新于 {album.updatedAt}
              </span>
            </div>
          </div>
        </button>
      ))}
    </div>
  );
}
