# Music Gift Demo (v0.12)

## Background

"Moment" is an AI music gift app: users chat with an LLM to craft personalized
song lyrics, then submit them to a music generation provider (Mureka) to
produce an audio track. The original implementation is a Node.js/Express app
using `pi-ai` for LLM access and direct `fetch` calls to the Mureka API.

This demo rebuilds Moment on the Orchest SDK, exercising:
- **Agent loop**: multi-turn chat with streaming, tool dispatch, structured
  output parsing (lyrics/style/title/vocal tags)
- **GenTask provider**: Mureka music generation (submit -> poll -> fetch)
- **Vision**: photo upload + `ContentBlock::Image` for the LLM to inspect
- **Session persistence**: chat history preserved across requests
- **TS frontend**: structured rewrite, not a copy of the original HTML soup

## Product

A user creates a music gift in three phases:

1. **Chat**: The user describes who the song is for and shares a specific
   memory. The LLM (driven by Orchest's `AgentRun`) asks 1-2 follow-up
   questions, then generates structured lyrics with style/title/vocal tags.
   The chat streams to the frontend via SSE.

2. **Music generation**: The lyrics + style are submitted to Mureka via the
   SDK's `GenTask` trait. The job runs asynchronously (submit -> poll ->
   fetch). The frontend polls a status endpoint.

3. **Gift**: The completed audio + lyrics + metadata are saved as a "gift"
   that can be shared, opened, played, and liked.

## Architecture

```
examples/demo/music-gift/
├── Cargo.toml
├── README.md
├── .env.example
├── src/
│   ├── main.rs              # axum server bootstrap
│   ├── config.rs            # env var loading, ProviderRuntimeConfig
│   ├── agent.rs             # AgentRun setup, system prompt, lyrics parsing
│   ├── tools.rs             # music_gen tool (GenTask wrapper), photo tool
│   ├── gift.rs              # Gift struct, SQLite store, CRUD
│   ├── routes.rs            # axum handlers: /api/chat, /api/gift, /api/generate
│   └── events.rs            # SSE event serialization
├── frontend/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── src/
│   │   ├── main.tsx         # React entry
│   │   ├── App.tsx          # router: create / gift / playlist
│   │   ├── api.ts           # typed API client (fetch + SSE)
│   │   ├── types.ts         # shared types matching Rust DTOs
│   │   ├── i18n.ts          # zh / en / fr / es / ru
│   │   ├── pages/
│   │   │   ├── CreatePage.tsx   # chat flow + lyrics review
│   │   │   ├── GiftPage.tsx     # gift view + audio player + share
│   │   │   └── PlaylistPage.tsx # public gift list
│   │   └── components/
│   │       ├── ChatStream.tsx   # SSE-driven chat display
│   │       ├── LyricsCard.tsx   # lyrics review with edit
│   │       └── AudioPlayer.tsx  # audio playback with progress
│   └── public/
└── tests/
    └── smoke.rs
```

## Backend (Rust + axum)

### Env vars

| Name | Required | Purpose |
|------|----------|---------|
| `MUSIC_GIFT_CHAT_MODEL` | yes | `provider/model` for LLM (e.g. `anthropic/claude-sonnet-4-6`) |
| `MUSIC_GIFT_CHAT_API_KEY` | no | explicit API key; falls back to provider default |
| `MUSIC_GIFT_MUSIC_PROVIDER` | yes | `mureka` (or `minimax`, `suno`, `aliyun`) |
| `MUSIC_GIFT_MUSIC_MODEL` | no | model id; defaults to provider default |
| `MUSIC_GIFT_MUSIC_API_KEY` | yes | music provider API key |
| `MUSIC_GIFT_MUSIC_API_URL` | no | override endpoint |
| `MUSIC_GIFT_PORT` | no | default 3000 |

### API endpoints

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/chat` | SSE stream: send messages, receive LLM tokens + structured lyrics |
| POST | `/api/gift` | Create gift (lyrics, meta, style, title, vocal) |
| GET | `/api/gift/:id` | Get gift data |
| GET | `/api/playlist` | List published gifts |
| POST | `/api/generate/:id` | Submit music generation job |
| GET | `/api/generate/:id/status` | Poll music generation status |
| POST | `/api/gift/:id/like` | Like a gift |
| POST | `/api/photos` | Upload photos (base64) |
| GET | `/audio/*` | Serve generated audio files |

### Agent loop

The chat endpoint uses `AgentRun::start` with:
- `ModelAdapter` from `create_adapter_from_config` (same as briefing-desk)
- System prompt: creative assistant that collects details, generates lyrics
  with `<<<LYRICS>>>` / `<<<STYLE>>>` / `<<<TITLE>>>` / `<<<VOCAL>>>` tags
- `max_steps(1)` per turn (single model call per request, no tool loop)
- Streaming via `StreamEvent` -> SSE

The LLM output is parsed for structured tags. If `<<<LYRICS>>>` is present,
the SSE final event includes `{ hasLyrics: true, lyrics, style, title, vocal }`.

### Music generation

Mureka (or other provider) is constructed via `Registry::with_builtin().gen()`
and wrapped as a Rust function (not an agent tool -- music gen is a direct
API call, not something the LLM decides to invoke). The flow:

1. `POST /api/generate/:id` -> `gen_task.submit(GenRequest { prompt, params })`
   -> returns job id, stores `GenHandle` in gift record
2. `GET /api/generate/:id/status` -> `gen_task.poll(&handle)` -> returns
   `GenStatus` (Pending / Running / Done / Failed)
3. When Done -> `gen_task.fetch(&handle)` -> stores audio URL in gift

### Gift store

SQLite via `orchest::session::SqliteSessionStore` pattern (or a standalone
`rusqlite` connection). Schema:

```sql
CREATE TABLE gifts (
  id TEXT PRIMARY KEY,
  kind TEXT,           -- 'song' | 'instrumental'
  lyrics TEXT,
  meta TEXT,           -- JSON: { name, relationship, style, title, vocal, lang, ... }
  audio_url TEXT,
  photos TEXT,         -- JSON array of paths
  gen_handle TEXT,     -- JSON: serialized GenHandle for polling
  gen_status TEXT,     -- 'pending' | 'running' | 'done' | 'failed'
  creator_token TEXT,
  published INTEGER,
  likes TEXT,          -- JSON array of viewer IDs
  created_at TEXT,
  published_at TEXT
);
```

## Frontend (React + TypeScript + Vite)

### Design principles

- **Structured**: React components with typed props, not inline HTML strings
- **Product-grade**: proper state management, loading states, error handling
- **i18n**: typed translations, not loose string dictionaries
- **Reuse design language**: same warm/gold palette, Cormorant Garamond font,
  noise texture as the original
- **SSE streaming**: EventSource API for chat, fetch polling for music gen

### Pages

1. **CreatePage**: Chat interface (SSE stream), meta form (name, relationship,
   scenario, lang), photo upload, lyrics review card with edit, generate
   button
2. **GiftPage**: Audio player, lyrics display, share button, like button
3. **PlaylistPage**: Grid of published gifts with play buttons

## Dependencies (Rust)

```toml
[dependencies]
orchest = { path = "../../../crates/orchest", features = ["sqlite-session"] }
orchest-protocol = { path = "../../../crates/orchest-protocol" }
orchest-provider = { path = "../../../crates/orchest-provider", features = ["llm", "gen"] }
tokio = { version = "1", features = ["full"] }
axum = "0.8"
tower-http = { version = "0.6", features = ["fs", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
rusqlite = { version = "0.32", features = ["bundled"] }
base64 = "0.22"
clap = { version = "4", features = ["derive"] }
```

## Dependencies (Frontend)

```json
{
  "dependencies": {
    "react": "^19",
    "react-dom": "^19",
    "react-router-dom": "^7"
  },
  "devDependencies": {
    "typescript": "^5",
    "vite": "^6",
    "@vitejs/plugin-react": "^4"
  }
}
```

## Acceptance Criteria

- [ ] `cargo build -p music-gift-demo` succeeds
- [ ] Chat endpoint streams LLM tokens via SSE
- [ ] Lyrics parsing extracts style/title/vocal tags correctly
- [ ] Music generation submits to Mureka and polls to completion
- [ ] Gift CRUD works (create, get, list, like)
- [ ] Photo upload + vision works (image blocks sent to LLM)
- [ ] Frontend builds with `npm run build`
- [ ] CreatePage: chat -> lyrics review -> generate -> gift flow works end-to-end
- [ ] GiftPage: audio playback, share, like
- [ ] PlaylistPage: list + play
- [ ] `cargo test -p music-gift-demo` passes (smoke tests)
- [ ] `cargo clippy -p music-gift-demo -- -D warnings` clean

## Non-Goals

- Payment/paywall (original had a stub; skip for demo)
- Analytics dashboard (original had event logging + dashboard; skip for demo)
- Countdown page generation (original had LLM-generated HTML scenes; skip)
- Remix flow (original had parent/child song remix; skip)
- Multi-language support beyond zh/en in initial version
