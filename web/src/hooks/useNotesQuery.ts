import { useQuery } from '@tanstack/react-query';
import { listNotesRequest } from '../api/notes_api';
import type { NoteFilters } from '../types/note';

export function useNotesQuery(filters: NoteFilters, demoState: 'default' | 'loading' | 'error' | 'empty') {
  return useQuery({
    queryKey: ['notes', filters, demoState],
    queryFn: async () => {
      const response = await listNotesRequest(filters, demoState);
      return response.data;
    },
  });
}
