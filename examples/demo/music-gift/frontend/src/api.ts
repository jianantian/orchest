// Typed API client for the Music Gift backend.
// SSE streaming uses fetch + ReadableStream (POST + SSE is not supported by EventSource).

import type {
  ChatRequest,
  CreateGiftRequest,
  CreateGiftResponse,
  GenerateResponse,
  Gift,
  LikeResponse,
  PlaylistResponse,
  SseEvent,
} from './types';

/**
 * Stream chat responses from the backend via SSE.
 * Yields parsed SseEvent objects (Delta / Done / Error).
 */
export async function* streamChat(req: ChatRequest): AsyncGenerator<SseEvent> {
  const res = await fetch('/api/chat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });

  if (!res.ok) {
    throw new Error(`Chat request failed: ${res.status}`);
  }
  if (!res.body) {
    throw new Error('No response body for chat stream');
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop() ?? '';
    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const json = line.slice(6);
        try {
          yield JSON.parse(json) as SseEvent;
        } catch {
          // Skip malformed JSON lines (keepalive, partial, etc.)
        }
      }
    }
  }

  // Process any remaining buffered data
  if (buffer.startsWith('data: ')) {
    try {
      yield JSON.parse(buffer.slice(6)) as SseEvent;
    } catch {
      // Ignore trailing malformed line
    }
  }
}

/** POST /api/gift — create a new gift record. */
export async function createGift(req: CreateGiftRequest): Promise<CreateGiftResponse> {
  const res = await fetch('/api/gift', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Create gift failed: ${res.status}`);
  return res.json() as Promise<CreateGiftResponse>;
}

/** GET /api/gift/:id — fetch a gift by id. */
export async function getGift(id: string): Promise<Gift> {
  const res = await fetch(`/api/gift/${id}`);
  if (!res.ok) throw new Error(`Get gift failed: ${res.status}`);
  return res.json() as Promise<Gift>;
}

/** GET /api/playlist — list all published gifts. */
export async function getPlaylist(): Promise<PlaylistResponse> {
  const res = await fetch('/api/playlist');
  if (!res.ok) throw new Error(`Get playlist failed: ${res.status}`);
  return res.json() as Promise<PlaylistResponse>;
}

/** GET /api/my-gifts — the signed-in creator's gifts, any device. */
export async function getMyGifts(): Promise<Gift[]> {
  const res = await fetch('/api/my-gifts');
  if (!res.ok) throw new Error(`Get my gifts failed: ${res.status}`);
  const body = (await res.json()) as { items: Gift[] };
  return body.items;
}

/** POST /api/gift/claim — attach device-local gifts to the signed-in account. */
export async function claimGifts(
  gifts: Array<{ id: string; creator_token: string }>,
): Promise<number> {
  const res = await fetch('/api/gift/claim', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ gifts }),
  });
  if (!res.ok) throw new Error(`Claim gifts failed: ${res.status}`);
  const body = (await res.json()) as { claimed: number };
  return body.claimed;
}

/** POST /api/auth/me/password — set a new password for the signed-in user. */
export async function setPassword(password: string): Promise<void> {
  const res = await fetch('/api/auth/me/password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ password }),
  });
  if (!res.ok) throw new Error(`Set password failed: ${res.status}`);
}

/** POST /api/auth/forgot — request a password-reset email. */
export async function forgotPassword(email: string): Promise<void> {
  const res = await fetch('/api/auth/forgot', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email }),
  });
  if (!res.ok) throw new Error(`Forgot password failed: ${res.status}`);
}

/** POST /api/auth/reset — redeem a reset token with a new password. */
export async function resetPassword(token: string, password: string): Promise<void> {
  const res = await fetch('/api/auth/reset', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token, password }),
  });
  if (!res.ok) throw new Error(`Reset password failed: ${res.status}`);
}

/** POST /api/generate/:id — submit a music generation job. Creator only. The
 *  token may be absent when the caller acts via a session (creator_id). */
export async function generateMusic(id: string, creatorToken?: string): Promise<GenerateResponse> {
  const res = await fetch(`/api/generate/${id}`, {
    method: 'POST',
    headers: creatorToken ? { 'X-Creator-Token': creatorToken } : undefined,
  });
  if (!res.ok) throw new Error(`Generate music failed: ${res.status}`);
  return res.json() as Promise<GenerateResponse>;
}

/** DELETE /api/gift/:id — permanently remove a gift. Creator only. Token may
 *  be absent when the caller acts via a session (creator_id). */
export async function deleteGift(id: string, creatorToken?: string): Promise<void> {
  const res = await fetch(`/api/gift/${id}`, {
    method: 'DELETE',
    headers: creatorToken ? { 'X-Creator-Token': creatorToken } : undefined,
  });
  if (!res.ok) throw new Error(`Delete gift failed: ${res.status}`);
}

/** POST /api/gift/:id/publish — list/unlist on the public playlist. Creator
 *  only. Token may be absent when the caller acts via a session. */
export async function setGiftPublished(
  id: string,
  creatorToken: string | undefined,
  published: boolean,
): Promise<void> {
  const res = await fetch(`/api/gift/${id}/publish`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(creatorToken ? { 'X-Creator-Token': creatorToken } : {}),
    },
    body: JSON.stringify({ published }),
  });
  if (!res.ok) throw new Error(`Publish toggle failed: ${res.status}`);
}

/** POST /api/gift/:id/like — like a gift (idempotent by viewer id). */
export async function likeGift(id: string, viewerId: string): Promise<LikeResponse> {
  const res = await fetch(`/api/gift/${id}/like`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ viewer_id: viewerId }),
  });
  if (!res.ok) throw new Error(`Like gift failed: ${res.status}`);
  return res.json() as Promise<LikeResponse>;
}

/** Statuses emitted by GET /api/generate/:id/stream (SSE). */
export type GenerationStatus = 'pending' | 'running' | 'done' | 'failed' | 'timeout';

export interface GenerationWatchHandlers {
  /** In-progress update. */
  onProgress?: (status: 'pending' | 'running') => void;
  /** Terminal success — carries the audio URL (null if the backend omitted it). */
  onDone?: (audioUrl: string | null) => void;
  /**
   * Terminal failure: the job failed or timed out, or the stream was lost
   * for good. The backend's bare `{"status":"error"}` (gift or gen handle
   * missing) is normalized to `failed`.
   */
  onFailed?: (reason: 'failed' | 'timeout' | 'connection-lost') => void;
}

export interface GenerationWatch {
  close: () => void;
}

/**
 * Watch a music generation job over SSE until a terminal status.
 *
 * The single owner of the /api/generate/:id/stream wire contract: EventSource
 * construction, JSON parsing, and the status union. Malformed events and
 * unknown statuses are ignored (keep waiting). Transient drops auto-reconnect
 * (EventSource default); only a permanently closed stream — e.g. a non-SSE
 * error response — reports `connection-lost`.
 */
export function watchGeneration(id: string, handlers: GenerationWatchHandlers): GenerationWatch {
  const es = new EventSource(`/api/generate/${id}/stream`);

  es.onmessage = (event) => {
    let data: { status?: unknown; audio_url?: unknown };
    try {
      data = JSON.parse(event.data);
    } catch {
      return; // Ignore malformed events
    }
    switch (data.status) {
      case 'pending':
      case 'running':
        handlers.onProgress?.(data.status);
        break;
      case 'done':
        es.close();
        handlers.onDone?.(typeof data.audio_url === 'string' ? data.audio_url : null);
        break;
      case 'failed':
      case 'error': // Sent when the gift or its gen handle is gone
      case 'timeout':
        es.close();
        handlers.onFailed?.(data.status === 'timeout' ? 'timeout' : 'failed');
        break;
      default:
        break; // Unknown status — keep waiting
    }
  };

  es.onerror = () => {
    if (es.readyState === EventSource.CLOSED) {
      handlers.onFailed?.('connection-lost');
    }
  };

  return { close: () => es.close() };
}
