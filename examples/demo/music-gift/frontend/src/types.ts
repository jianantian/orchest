// Type definitions matching the Rust backend DTOs.

/** A chat message sent to / received from the backend. */
export interface ChatMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

/** Metadata for a gift: recipient info + generated song attributes. */
export interface GiftMeta {
  name?: string;
  relationship?: string;
  scenario?: string;
  lang?: string;
  style?: string;
  title?: string;
  vocal?: string;
  model?: string;
  [key: string]: unknown;
}

/** Request body for POST /api/chat (SSE streaming). */
export interface ChatRequest {
  messages: ChatMessage[];
  meta: GiftMeta;
  lang?: string;
  photos: string[];
}

/** SSE events from POST /api/chat — tagged union via `type` discriminator. */
export type SseEvent =
  | { type: 'Delta'; text: string }
  | {
      type: 'Done';
      has_lyrics: boolean;
      lyrics: string;
      style: string;
      title: string;
      vocal: string;
    }
  | { type: 'Error'; error: string };

/** A complete gift record from GET /api/gift/:id. */
export interface Gift {
  id: string;
  kind: string;
  lyrics: string | null;
  meta: GiftMeta;
  audio_url: string | null;
  photos: string[];
  gen_handle: string | null;
  gen_status: string | null;
  creator_token: string;
  published: boolean;
  likes: string[];
  created_at: string;
  published_at: string | null;
  countdown_status?: string | null;
  lrc?: string | null;
  duration_secs?: number | null;
}

/** Response from POST /api/gift. */
export interface CreateGiftResponse {
  id: string;
  creator_token: string;
}

/** Request body for POST /api/gift. */
export interface CreateGiftRequest {
  lyrics?: string;
  kind?: string;
  meta?: GiftMeta;
  photos?: string[];
  style?: string;
}

/** A single item in the playlist (GET /api/playlist). */
export interface PlaylistItem {
  id: string;
  title: string;
  name: string;
  relationship: string;
  style: string;
  lang: string;
  audio_url: string | null;
  likes: number;
  published_at: string | null;
}

/** Response from GET /api/playlist. */
export interface PlaylistResponse {
  items: PlaylistItem[];
}

/** Response from POST /api/generate/:id. */
export interface GenerateResponse {
  id: string;
  status: string;
  handle: string | null;
}

/** Response from GET /api/generate/:id/status. */
export interface GenStatusResponse {
  id: string;
  status: 'pending' | 'running' | 'done' | 'failed';
  audio_url: string | null;
}

/** Response from POST /api/gift/:id/like. */
export interface LikeResponse {
  ok: boolean;
  likes: number;
  liked: boolean;
}

/** Response from POST /api/photos. */
export interface PhotoUploadResponse {
  urls: string[];
}
