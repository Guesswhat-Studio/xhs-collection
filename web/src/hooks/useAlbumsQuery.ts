import { useQuery } from '@tanstack/react-query';
import { getAlbumNotesRequest, listAlbumsRequest } from '../api/albums_api';

export function useAlbumsQuery() {
  return useQuery({
    queryKey: ['albums'],
    queryFn: async () => {
      const response = await listAlbumsRequest();
      return response.data;
    },
  });
}

export function useAlbumNotesQuery(albumId?: string) {
  return useQuery({
    queryKey: ['albums', albumId, 'notes'],
    enabled: Boolean(albumId),
    queryFn: async () => {
      const response = await getAlbumNotesRequest(albumId!);
      return response.data;
    },
  });
}
