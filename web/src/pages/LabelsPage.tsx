import { useMemo, useState } from 'react';
import { LabelList } from '../components/labels/LabelList';
import { LabelUsageTable } from '../components/labels/LabelUsageTable';
import { useLabelsQuery } from '../hooks/useLabelsQuery';
import { useNotesQuery } from '../hooks/useNotesQuery';

export function LabelsPage() {
  const labelsQuery = useLabelsQuery();
  const notesQuery = useNotesQuery(
    {
      searchText: '',
      staleness: 'all',
      accountName: 'all',
      label: 'all',
      showArchived: true,
    },
    'default',
  );
  const [activeLabel, setActiveLabel] = useState<string | null>(null);

  const filteredNotes = useMemo(() => {
    const notes = notesQuery.data?.notes ?? [];
    if (!activeLabel) {
      return notes;
    }

    return notes.filter((note) => [...note.systemLabels, ...note.userLabels].includes(activeLabel));
  }, [activeLabel, notesQuery.data?.notes]);

  return (
    <section className="stack-page">
      <div className="page-intro panel-surface">
        <p className="panel-header__eyebrow">Label maintenance</p>
        <h3>标签不是越多越好，而是越接近你回看时脑子里的分类方式越有用</h3>
        <p>系统标签让东西先有地方落，用户标签负责把“以后会怎么找它”这件事说清楚。</p>
      </div>

      <div className="stack-layout">
        <LabelList labels={labelsQuery.data ?? []} activeLabel={activeLabel} onSelect={setActiveLabel} />
        <LabelUsageTable labels={labelsQuery.data ?? []} notes={filteredNotes} />
      </div>
    </section>
  );
}
