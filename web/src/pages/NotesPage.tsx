import { LayoutGrid, List, RefreshCcw, SearchCheck, SlidersHorizontal } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { NoteDetailPanel } from '../components/notes/NoteDetailPanel';
import { NoteFilters } from '../components/notes/NoteFilters';
import { NoteList } from '../components/notes/NoteList';
import { useAlbumsQuery } from '../hooks/useAlbumsQuery';
import { useLabelsQuery } from '../hooks/useLabelsQuery';
import { useNotesQuery } from '../hooks/useNotesQuery';
import { useBulkNoteAction, useUpdateNote } from '../hooks/useUpdateNote';
import type { NoteFilters as NoteFiltersState, NoteViewMode } from '../types/note';

const defaultFilters: NoteFiltersState = {
  searchText: '',
  staleness: 'all',
  accountName: 'all',
  label: 'all',
  showArchived: false,
};

const emptyNotes: NonNullable<ReturnType<typeof useNotesQuery>['data']>['notes'] = [];
const emptyAccounts: string[] = [];
const emptySuggestedLabels: string[] = [];

const demoStates = [
  { value: 'default', label: 'Primary' },
  { value: 'loading', label: 'Loading' },
  { value: 'error', label: 'Error' },
  { value: 'empty', label: 'Empty' },
] as const;

export function NotesPage() {
  const [filters, setFilters] = useState<NoteFiltersState>(defaultFilters);
  const [viewMode, setViewMode] = useState<NoteViewMode>('grid');
  const [demoState, setDemoState] = useState<(typeof demoStates)[number]['value']>('default');
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [activeNoteId, setActiveNoteId] = useState<string | null>(null);

  const notesQuery = useNotesQuery(filters, demoState);
  const albumsQuery = useAlbumsQuery();
  const labelsQuery = useLabelsQuery();
  const updateNote = useUpdateNote();
  const bulkAction = useBulkNoteAction();

  const notes = notesQuery.data?.notes ?? emptyNotes;
  const accounts = notesQuery.data?.accounts ?? emptyAccounts;
  const suggestedLabels = notesQuery.data?.suggestedLabels ?? emptySuggestedLabels;
  const albums = albumsQuery.data ?? [];
  const labels = labelsQuery.data ?? [];

  useEffect(() => {
    if (!notes.length) {
      if (activeNoteId !== null) {
        setActiveNoteId(null);
      }
      setSelectedIds((current) => (current.length ? [] : current));
      return;
    }

    if (!activeNoteId || !notes.some((note) => note.id === activeNoteId)) {
      setActiveNoteId(notes[0].id);
    }

    setSelectedIds((current) => current.filter((id) => notes.some((note) => note.id === id)));
  }, [activeNoteId, notes]);

  const activeNote = useMemo(() => notes.find((note) => note.id === activeNoteId) ?? null, [activeNoteId, notes]);

  const visibleAllSelected = notes.length > 0 && notes.every((note) => selectedIds.includes(note.id));

  const handleToggleSelect = (id: string) => {
    setSelectedIds((current) => (current.includes(id) ? current.filter((item) => item !== id) : [...current, id]));
  };

  const handleSelectAll = () => {
    setSelectedIds(visibleAllSelected ? [] : notes.map((note) => note.id));
  };

  const handleBulkAction = async (action: 'archive' | 'restore' | 'tag-focus' | 'album-weekend') => {
    if (!selectedIds.length) {
      return;
    }

    await bulkAction.mutateAsync({ ids: selectedIds, action });
    if (action === 'archive' || action === 'restore') {
      setSelectedIds([]);
    }
  };

  const handlePatchNote = async (patch: { userNote?: string; userLabels?: string[]; albumIds?: string[]; archived?: boolean }) => {
    if (!activeNote) {
      return;
    }

    await updateNote.mutateAsync({ id: activeNote.id, ...patch });
  };

  const handleAddLabel = async (label: string) => {
    if (!activeNote) {
      return;
    }

    const next = Array.from(new Set([...activeNote.userLabels, label]));
    await handlePatchNote({ userLabels: next });
  };

  const handleToggleAlbum = async (albumId: string) => {
    if (!activeNote) {
      return;
    }

    const next = activeNote.albumIds.includes(albumId)
      ? activeNote.albumIds.filter((item) => item !== albumId)
      : [...activeNote.albumIds, albumId];

    await handlePatchNote({ albumIds: next });
  };

  return (
    <section className="notes-page">
      <div className="notes-page__hero panel-surface">
        <div>
          <p className="panel-header__eyebrow">Core workspace</p>
          <h3>把“我记得收藏过，但忘了在哪”变成可以马上处理的范围</h3>
          <p>
            默认从近期同步和常用标签开始。筛选负责缩小范围，右侧抽屉负责不打断上下文地完成整理。
          </p>
        </div>

        <div className="notes-page__hero-actions">
          <div className="state-toggle" aria-label="Visual states">
            {demoStates.map((state) => (
              <button
                key={state.value}
                type="button"
                className={`chip-button${demoState === state.value ? ' is-selected' : ''}`}
                onClick={() => setDemoState(state.value)}
              >
                {state.label}
              </button>
            ))}
          </div>
          <button type="button" className="secondary-button" onClick={() => notesQuery.refetch()}>
            <RefreshCcw size={14} />
            重新载入
          </button>
        </div>
      </div>

      <div className="notes-workbench">
        <NoteFilters
          accounts={accounts}
          labels={labels}
          suggestedLabels={suggestedLabels}
          filters={filters}
          onChange={(patch) => setFilters((current) => ({ ...current, ...patch }))}
        />

        <section className="notes-workbench__main panel-surface">
          <div className="notes-workbench__toolbar">
            <div>
              <p className="panel-header__eyebrow">Browse & organize</p>
              <h3>
                <SearchCheck size={16} />
                {notesQuery.isError ? '这次先没拿到结果' : `${notes.length} 条正在当前视角里`}
              </h3>
            </div>

            <div className="notes-workbench__toolbar-actions">
              <button type="button" className="secondary-button" onClick={handleSelectAll}>
                <SlidersHorizontal size={14} />
                {visibleAllSelected ? '清除本屏选择' : '选择本屏全部'}
              </button>
              <div className="view-toggle">
                <button
                  type="button"
                  className={`view-toggle__button${viewMode === 'grid' ? ' is-selected' : ''}`}
                  onClick={() => setViewMode('grid')}
                >
                  <LayoutGrid size={15} />
                  Card
                </button>
                <button
                  type="button"
                  className={`view-toggle__button${viewMode === 'list' ? ' is-selected' : ''}`}
                  onClick={() => setViewMode('list')}
                >
                  <List size={15} />
                  List
                </button>
              </div>
            </div>
          </div>

          {selectedIds.length > 0 && (
            <div className="bulk-bar">
              <div>
                <strong>{selectedIds.length}</strong>
                <span>条已选，可以继续批量整理</span>
              </div>
              <div className="bulk-bar__actions">
                <button type="button" className="secondary-button" onClick={() => handleBulkAction('tag-focus')}>
                  标记待复盘
                </button>
                <button type="button" className="secondary-button" onClick={() => handleBulkAction('album-weekend')}>
                  加入低消耗周末
                </button>
                <button type="button" className="secondary-button" onClick={() => handleBulkAction('archive')}>
                  批量归档
                </button>
                <button type="button" className="ghost-button" onClick={() => setSelectedIds([])}>
                  清空选择
                </button>
              </div>
            </div>
          )}

          {notesQuery.isLoading ? (
            <div className="notes-skeleton" aria-hidden="true">
              {Array.from({ length: 6 }).map((_, index) => (
                <div key={index} className="notes-skeleton__card" />
              ))}
            </div>
          ) : notesQuery.isError ? (
            <div className="empty-state panel-surface empty-state--error">
              <div className="empty-state__icon">
                <RefreshCcw size={18} />
              </div>
              <h3>同步摘要这次没有顺利回来</h3>
              <p>{notesQuery.error instanceof Error ? notesQuery.error.message : '可以重新刷新，或者先切回 Primary 状态继续看默认场景。'}</p>
              <button type="button" className="primary-button" onClick={() => setDemoState('default')}>
                回到默认状态
              </button>
            </div>
          ) : (
            <NoteList
              notes={notes}
              viewMode={viewMode}
              activeNoteId={activeNoteId}
              selectedIds={selectedIds}
              onActivate={setActiveNoteId}
              onSelectToggle={handleToggleSelect}
            />
          )}
        </section>

        <NoteDetailPanel
          note={activeNote}
          albums={albums}
          saving={updateNote.isPending}
          onSaveNote={handlePatchNote}
          onAddLabel={handleAddLabel}
          onToggleAlbum={handleToggleAlbum}
          onToggleArchived={() => handlePatchNote({ archived: !(activeNote?.archived ?? false) })}
        />
      </div>
    </section>
  );
}
