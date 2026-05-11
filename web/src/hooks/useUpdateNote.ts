import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createAlbumRequest } from '../api/albums_api';
import { bulkNotesActionRequest, updateNoteRequest } from '../api/notes_api';
import type { CreateAlbumInput } from '../types/album';
import type { BulkNoteActionInput, UpdateNoteInput } from '../types/note';

export function useUpdateNote() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: UpdateNoteInput) => {
      const response = await updateNoteRequest(input);
      return response.data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notes'] });
      queryClient.invalidateQueries({ queryKey: ['albums'] });
      queryClient.invalidateQueries({ queryKey: ['labels'] });
      queryClient.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}

export function useBulkNoteAction() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: BulkNoteActionInput) => {
      const response = await bulkNotesActionRequest(input);
      return response.data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notes'] });
      queryClient.invalidateQueries({ queryKey: ['albums'] });
      queryClient.invalidateQueries({ queryKey: ['labels'] });
      queryClient.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}

export function useCreateAlbum() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: CreateAlbumInput) => {
      const response = await createAlbumRequest(input);
      return response.data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['albums'] });
      queryClient.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}
