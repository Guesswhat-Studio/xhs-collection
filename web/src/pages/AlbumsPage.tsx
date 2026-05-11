import { useEffect, useState } from 'react';
import { AlbumEditor } from '../components/albums/AlbumEditor';
import { AlbumList } from '../components/albums/AlbumList';
import { AlbumNotesView } from '../components/albums/AlbumNotesView';
import { useAlbumNotesQuery, useAlbumsQuery } from '../hooks/useAlbumsQuery';
import { useCreateAlbum } from '../hooks/useUpdateNote';

export function AlbumsPage() {
  const albumsQuery = useAlbumsQuery();
  const createAlbum = useCreateAlbum();
  const [activeAlbumId, setActiveAlbumId] = useState<string | null>(null);

  useEffect(() => {
    if (!albumsQuery.data?.length) {
      setActiveAlbumId(null);
      return;
    }

    if (!activeAlbumId || !albumsQuery.data.some((album) => album.id === activeAlbumId)) {
      setActiveAlbumId(albumsQuery.data[0].id);
    }
  }, [activeAlbumId, albumsQuery.data]);

  const activeAlbum = albumsQuery.data?.find((album) => album.id === activeAlbumId) ?? null;
  const albumNotesQuery = useAlbumNotesQuery(activeAlbumId ?? undefined);

  return (
    <section className="stack-page">
      <div className="page-intro panel-surface">
        <p className="panel-header__eyebrow">Theme organization</p>
        <h3>专辑更像一个会继续长大的主题抽屉，而不是一次性的收藏夹</h3>
        <p>这里不强调“收了多少”，而强调“以后回看时有没有一个更自然的入口”。</p>
      </div>

      <div className="albums-layout">
        <div className="stack-layout">
          <AlbumEditor
            creating={createAlbum.isPending}
            onCreate={(payload) => {
              createAlbum.mutate(payload);
            }}
          />
          <section className="panel-surface">
            <div className="panel-header">
              <div>
                <p className="panel-header__eyebrow">Album library</p>
                <h3>{albumsQuery.data?.length ?? 0} 个专辑已经可以接住后续整理</h3>
              </div>
            </div>
            <AlbumList albums={albumsQuery.data ?? []} activeAlbumId={activeAlbumId} onSelect={setActiveAlbumId} />
          </section>
        </div>

        <AlbumNotesView album={activeAlbum} notes={albumNotesQuery.data ?? []} />
      </div>
    </section>
  );
}
