import { withEnvelope } from './client';
import { createAlbum, getAlbumNotes, listAlbums } from './mock_store';
import type { ApiEnvelope } from '../types/api';
import type { Album, CreateAlbumInput } from '../types/album';
import type { Note } from '../types/note';

export async function listAlbumsRequest(): Promise<ApiEnvelope<Album[]>> {
  return withEnvelope(() => listAlbums(), { delay: 220 });
}

export async function createAlbumRequest(input: CreateAlbumInput): Promise<ApiEnvelope<Album>> {
  return withEnvelope(() => createAlbum(input), { delay: 200 });
}

export async function getAlbumNotesRequest(albumId: string): Promise<ApiEnvelope<Note[]>> {
  return withEnvelope(() => getAlbumNotes(albumId), { delay: 180 });
}
