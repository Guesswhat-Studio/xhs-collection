import { useState } from 'react';
import type { Album } from '../../types/album';

interface AlbumEditorProps {
  creating: boolean;
  onCreate: (payload: { name: string; description: string; tone: Album['tone'] }) => void;
}

const tones: Album['tone'][] = ['travel', 'food', 'beauty', 'planning'];

export function AlbumEditor({ creating, onCreate }: AlbumEditorProps) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [tone, setTone] = useState<Album['tone']>('planning');

  return (
    <section className="panel-surface album-editor">
      <div className="panel-header">
        <div>
          <p className="panel-header__eyebrow">Quick create</p>
          <h3>先建一个能承接后续整理的专辑</h3>
        </div>
      </div>

      <div className="album-editor__fields">
        <input
          className="input-field"
          type="text"
          placeholder="例如：东京雨天备选路线"
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
        <textarea
          className="textarea-field"
          rows={3}
          placeholder="写一句以后回看时会立刻知道这个专辑为什么存在的话。"
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
        <div className="chip-grid">
          {tones.map((item) => (
            <button
              key={item}
              type="button"
              className={`chip-button${tone === item ? ' is-selected' : ''}`}
              onClick={() => setTone(item)}
            >
              {item}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="primary-button"
          disabled={creating || !name.trim()}
          onClick={() => {
            if (!name.trim()) {
              return;
            }
            onCreate({ name: name.trim(), description: description.trim(), tone });
            setName('');
            setDescription('');
            setTone('planning');
          }}
        >
          {creating ? '创建中…' : '新建专辑'}
        </button>
      </div>
    </section>
  );
}
