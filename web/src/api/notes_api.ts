import { withEnvelope } from './client';
import { applyBulkAction, getAccounts, getNote, getSuggestedLabels, listNotes, updateNote } from './mock_store';
import type { ApiEnvelope } from '../types/api';
import type { BulkNoteActionInput, Note, NoteFilters, UpdateNoteInput } from '../types/note';

export async function listNotesRequest(
  filters: NoteFilters,
  demoState: 'default' | 'loading' | 'error' | 'empty',
): Promise<ApiEnvelope<{ notes: Note[]; accounts: string[]; suggestedLabels: string[] }>> {
  if (demoState === 'error') {
    throw new Error('今天的同步摘要没有载入成功，请重新刷新一次看看。');
  }

  return withEnvelope(
    () => ({
      notes: demoState === 'empty' ? [] : listNotes(filters),
      accounts: getAccounts(),
      suggestedLabels: getSuggestedLabels(),
    }),
    {
      delay: demoState === 'loading' ? 1200 : 260,
      meta: { total: listNotes(filters).length },
    },
  );
}

export async function getNoteRequest(id: string): Promise<ApiEnvelope<Note | null>> {
  return withEnvelope(() => getNote(id), { delay: 120 });
}

export async function updateNoteRequest(input: UpdateNoteInput): Promise<ApiEnvelope<Note>> {
  return withEnvelope(() => updateNote(input), { delay: 180 });
}

export async function bulkNotesActionRequest(input: BulkNoteActionInput) {
  return withEnvelope(() => applyBulkAction(input), { delay: 220 });
}
