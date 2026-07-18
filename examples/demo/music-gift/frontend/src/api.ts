// Typed API client for the Music Gift backend.
// SSE streaming uses fetch + ReadableStream (POST + SSE is not supported by EventSource).

import type {
  ChatRequest,
  CreateGiftRequest,
  CreateGiftResponse,
  GenStatusResponse,
  GenerateResponse,
  Gift,
  LikeResponse,
  PhotoUploadResponse,
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

/** POST /api/generate/:id — submit a music generation job. */
export async function generateMusic(id: string): Promise<GenerateResponse> {
  const res = await fetch(`/api/generate/${id}`, { method: 'POST' });
  if (!res.ok) throw new Error(`Generate music failed: ${res.status}`);
  return res.json() as Promise<GenerateResponse>;
}

/** GET /api/generate/:id/status — poll music generation status. */
export async function pollGenerateStatus(id: string): Promise<GenStatusResponse> {
  const res = await fetch(`/api/generate/${id}/status`);
  if (!res.ok) throw new Error(`Poll status failed: ${res.status}`);
  return res.json() as Promise<GenStatusResponse>;
}

/** DELETE /api/gift/:id — permanently remove a gift. Creator only. */
export async function deleteGift(id: string, creatorToken: string): Promise<void> {
  const res = await fetch(`/api/gift/${id}`, {
    method: 'DELETE',
    headers: { 'X-Creator-Token': creatorToken },
  });
  if (!res.ok) throw new Error(`Delete gift failed: ${res.status}`);
}

/** POST /api/gift/:id/publish — list/unlist on the public playlist. Creator only. */
export async function setGiftPublished(
  id: string,
  creatorToken: string,
  published: boolean,
): Promise<void> {
  const res = await fetch(`/api/gift/${id}/publish`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-Creator-Token': creatorToken },
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

/** POST /api/photos — upload base64 photos, returns server paths. */
export async function uploadPhotos(dataUrls: string[]): Promise<PhotoUploadResponse> {
  const res = await fetch('/api/photos', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photos: dataUrls }),
  });
  if (!res.ok) throw new Error(`Upload photos failed: ${res.status}`);
  return res.json() as Promise<PhotoUploadResponse>;
}
