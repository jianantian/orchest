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
  /** Server-recorded stages that were skipped during generation (e.g. ["music_prompt"]). */
  degraded?: string[];
  [key: string]: unknown;
}

/** Request body for POST /api/chat (SSE streaming). */
export interface ChatRequest {
  messages: ChatMessage[];
  meta: GiftMeta;
  lang?: string;
  photos: string[];
  /** Collaboration mode: "studio" switches to the co-editing prompt and
   *  skips the elevate/review pipeline. Absent = guided mode. */
  mode?: string;
  /** The current working draft; only meaningful in studio mode. An absent
   *  field means "no value yet", not "cleared". */
  draft?: {
    lyrics?: string;
    style?: string;
    title?: string;
    vocal?: string;
  };
}

export type SseEvent =
  | { type: 'Delta'; text: string }
  /** Emitted when the creative elevation pass starts (after Deltas end, before Reviewing). */
  | { type: 'Elevating' }
  /** Emitted when the lyric review pass starts (after Deltas end, before Done). */
  | { type: 'Reviewing' }
  | {
      type: 'Done';
      has_lyrics: boolean;
      /** `null` = the turn did not (re-)emit this field. Studio clients apply
       *  only the non-null fields; guided turns always send non-null values. */
      lyrics: string | null;
      style: string | null;
      title: string | null;
      vocal: string | null;
      review?: string;
      /** Pipeline stages the server fell back on this turn (e.g. ["review"]). */
      degraded?: string[];
    }
  | { type: 'Error'; error: string };

/** A complete gift record from GET /api/gift/:id. */
export interface Gift {
  id: string;
  kind: string;
  lyrics: string | null;
  meta: GiftMeta;
  audio_url: string | null;
  cover_url?: string | null;
  photos: string[];
  gen_handle: string | null;
  gen_status: string | null;
  /** Account the gift belongs to (null until claimed / created logged in). */
  creator_id: string | null;
  published: boolean;
  likes: string[];
  created_at: string;
  published_at: string | null;
  countdown_status?: string | null;
  lrc?: string | null;
  duration_secs?: number | null;
}

/** One generation snapshot of a gift (GET /api/gift/:id/versions, newest
 *  first). `gen_request` never leaves the backend. */
export interface GiftVersion {
  gift_id: string;
  version: number;
  lyrics: string | null;
  meta: GiftMeta;
  audio_url: string | null;
  cover_url: string | null;
  lrc: string | null;
  duration_secs: number | null;
  created_at: string;
}

/** Request body for PATCH /api/gift/:id — absent fields stay untouched and
 *  no generation is triggered. */
export interface GiftFieldUpdates {
  lyrics?: string;
  title?: string;
  style?: string;
  vocal?: string;
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

/** Response from POST /api/gift/:id/like. */
export interface LikeResponse {
  ok: boolean;
  likes: number;
  liked: boolean;
}
