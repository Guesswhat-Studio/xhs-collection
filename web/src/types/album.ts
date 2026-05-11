export interface Album {
  id: string;
  name: string;
  description: string;
  tone: 'travel' | 'food' | 'beauty' | 'planning';
  updatedAt: string;
  noteIds: string[];
}

export interface CreateAlbumInput {
  name: string;
  description: string;
  tone: Album['tone'];
}
