import { useState } from 'react';
import type { SseEvent } from '../types';

interface LyricsCardProps {
  doneEvent: Extract<SseEvent, { type: 'Done' }>;
  onCreate: (lyrics: string, style: string, title: string, vocal: string) => void;
  creating: boolean;
}

export default function LyricsCard({ doneEvent, onCreate, creating }: LyricsCardProps) {
  const [editing, setEditing] = useState(false);
  const [lyrics, setLyrics] = useState(doneEvent.lyrics);
  const [style] = useState(doneEvent.style);
  const [title] = useState(doneEvent.title);
  const [vocal] = useState(doneEvent.vocal);

  return (
    <div className="card lyrics-card">
      <div className="lyrics-header">
        <h3 className="lyrics-title-display">{title || 'Your Song'}</h3>
        <div className="lyrics-tags">
          {style && <span className="lyrics-tag">♪ {style}</span>}
          {vocal && <span className="lyrics-tag">{vocal}</span>}
        </div>
      </div>

      {editing ? (
        <textarea
          className="form-textarea lyrics-edit"
          value={lyrics}
          onChange={(e) => setLyrics(e.target.value)}
          rows={12}
        />
      ) : (
        <pre className="lyrics-body">{lyrics}</pre>
      )}

      <div className="lyrics-actions">
        <button
          className="btn btn-secondary"
          onClick={() => setEditing(!editing)}
          disabled={creating}
        >
          {editing ? 'Preview' : 'Edit'}
        </button>
        <button
          className="btn btn-primary"
          onClick={() => onCreate(lyrics, style, title, vocal)}
          disabled={creating}
        >
          {creating ? (
            <>
              <span className="spinner" /> Creating…
            </>
          ) : (
            'Create Gift'
          )}
        </button>
      </div>
    </div>
  );
}
