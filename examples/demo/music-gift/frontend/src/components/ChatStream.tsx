import { useRef, useState, useEffect } from 'react';
import type { ChatMessage, GiftMeta, SseEvent } from '../types';
import { streamChat } from '../api';

interface ChatStreamProps {
  messages: ChatMessage[];
  setMessages: (msgs: ChatMessage[]) => void;
  meta: GiftMeta;
  photos: string[];
  onDone: (event: Extract<SseEvent, { type: 'Done' }>) => void;
}

export default function ChatStream({ messages, setMessages, meta, photos, onDone }: ChatStreamProps) {
  const [input, setInput] = useState('');
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom whenever messages change
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  async function handleSend() {
    const text = input.trim();
    if (!text || streaming) return;

    setInput('');
    setError(null);
    setStreaming(true);

    const userMsg: ChatMessage = { role: 'user', content: text };
    const newMessages = [...messages, userMsg];
    setMessages(newMessages);

    let assistantText = '';

    try {
      const gen = streamChat({
        messages: newMessages,
        meta,
        photos,
      });

      for await (const event of gen) {
        if (event.type === 'Delta') {
          assistantText += event.text;
          setMessages([...newMessages, { role: 'assistant', content: assistantText }]);
        } else if (event.type === 'Done') {
          if (event.has_lyrics) {
            onDone(event);
          }
        } else if (event.type === 'Error') {
          setError(event.error);
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Stream failed');
    } finally {
      setStreaming(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  }

  return (
    <div className="chat-stream">
      <div className="chat-messages" ref={scrollRef}>
        {messages.map((msg, i) => (
          <div key={i} className={`chat-bubble chat-${msg.role}`}>
            {msg.content || (msg.role === 'assistant' && streaming ? '…' : '')}
            {msg.role === 'assistant' && streaming && i === messages.length - 1 && msg.content && (
              <span className="chat-cursor" />
            )}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>

      {error && <div className="error-msg">{error}</div>}

      <div className="chat-input-row">
        <textarea
          className="chat-input"
          placeholder="Tell me about the person and a special memory…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          rows={1}
          disabled={streaming}
        />
        <button
          className="btn btn-primary chat-send"
          onClick={() => void handleSend()}
          disabled={!input.trim() || streaming}
        >
          {streaming ? <span className="spinner" /> : 'Send'}
        </button>
      </div>
    </div>
  );
}
