import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import type { ChatMessage, GiftMeta, SseEvent } from '../types';
import { createGift } from '../api';
import ChatStream from '../components/ChatStream';
import LyricsCard from '../components/LyricsCard';
import MetaForm from '../components/MetaForm';

type DoneEvent = Extract<SseEvent, { type: 'Done' }>;

export default function CreatePage() {
  const navigate = useNavigate();
  const [meta, setMeta] = useState<GiftMeta>({ lang: 'en' });
  const [photos, setPhotos] = useState<string[]>([]);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [doneEvent, setDoneEvent] = useState<DoneEvent | null>(null);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleCreate(lyrics: string, style: string, title: string, vocal: string) {
    setCreating(true);
    setError(null);
    try {
      const fullMeta: GiftMeta = { ...meta, style, title, vocal };
      const res = await createGift({
        lyrics,
        kind: 'song',
        meta: fullMeta,
        photos,
        style,
      });
      navigate(`/gift/${res.id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to create gift');
    } finally {
      setCreating(false);
    }
  }

  function handleDone(event: DoneEvent) {
    setDoneEvent(event);
  }

  return (
    <div className="create-page">
      {!doneEvent && (
        <>
          <MetaForm meta={meta} setMeta={setMeta} photos={photos} setPhotos={setPhotos} />
          <ChatStream
            messages={messages}
            setMessages={setMessages}
            meta={meta}
            photos={photos}
            onDone={handleDone}
          />
        </>
      )}

      {doneEvent && (
        <LyricsCard doneEvent={doneEvent} onCreate={handleCreate} creating={creating} />
      )}

      {error && <div className="error-msg">{error}</div>}
    </div>
  );
}
