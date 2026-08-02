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
│   │   │   ├── MyGiftsPage.tsx  # every gift created on this device
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
| POST | `/api/gift/claim` | Attach device-local gifts to the signed-in account (creator-token proof) |
| GET | `/api/my-gifts` | The signed-in creator's gifts (any device, incl. unpublished) |
| GET | `/api/playlist` | List published gifts |
| POST | `/api/generate/:id` | Submit music generation job |
| GET | `/api/generate/:id/status` | Poll music generation status |
| POST | `/api/gift/:id/like` | Like a gift |
| POST | `/api/photos` | Upload photos (base64) |
| GET | `/audio/*` | Serve generated audio files |

### Endpoint auth matrix

Three access levels. The demo is deliberately reachable by anonymous
visitors — read the assumptions at the bottom before exposing it beyond
localhost.

| Access | Endpoints |
|--------|-----------|
| Public (no auth) | `POST /api/chat`, `POST /api/polish-music-prompt`, `POST /api/gift`, `GET /api/gift/:id`, `GET /api/gift/:id/lrc`, `POST /api/gift/:id/like`, `GET /api/playlist`, `GET /api/generate/:id/status`, `GET /api/generate/:id/stream`, `POST /api/photos`, `GET /api/countdown-section/:id`, `GET /audio/*`, plus the auth routes (`register` / `login` / `forgot` / `reset` / OAuth) |
| Creator (either the `X-Creator-Token` header minted at creation, **or** a session whose user matches the gift's `creator_id`) | `POST /api/generate/:id`, `POST /api/gift/:id/publish`, `DELETE /api/gift/:id` |
| Session cookie (`session_token`; set by register/login/reset/OAuth) | `GET /api/auth/me`, `POST /api/auth/logout`, `POST /api/auth/me/password`, `GET /api/my-gifts`, `POST /api/gift/claim`; a valid session also links newly created gifts to the user (`creator_id`) |

Assumptions:

- **Anonymous LLM endpoints are a product decision, not an oversight.**
  `POST /api/chat` and `POST /api/polish-music-prompt` spend LLM quota
  without any auth so visitors can write lyrics before signing up. Anyone
  who can reach the server can burn model quota — put the demo behind a
  network boundary or a rate limiter if that matters for a deployment.
- `POST /api/generate/:id` is creator-only because it spends paid music
  provider quota against an existing gift. It is also idempotent: posting
  again while the gift is `pending` / `running` / `done` returns the current
  state instead of submitting a duplicate job.
- A gift is readable by anyone holding its id — that is how share links
  work. The `creator_token` is returned exactly once (at gift creation) and
  is never echoed by `GET /api/gift/:id`.
- **Account ownership is the cross-device story.** Mutations accept either
  the device-local `creator_token` or a session matching `creator_id`, so a
  signed-in creator can manage their gifts from any device. Gifts created
  while anonymous are claimed to the account by `POST /api/gift/claim`,
  using the stored tokens as proof; the frontend runs the claim
  automatically after login, and the My Gifts page merges account gifts
  with device-token gifts. Login endpoints are rate-limited per email
  (10 attempts / 15 min; reset links 5 / hour) — a demo-grade stop against
  brute force and mail spam.
- **Password reset is the standard email flow.** `POST /api/auth/forgot`
  emails a reset link (single-use, 30-minute token; answer is
  enumeration-proof — identical whether or not the email exists) and
  `POST /api/auth/reset` redeems it with a new password, invalidating every
  session minted under the old one. In local dev without `SMTP_PASS` the
  link is printed to the server log (`[auth] dev mode, reset link for ...`)
  so the flow stays testable before SMTP exists.
- Google OAuth uses a per-request random `state` bound to a cookie
  (login-CSRF protection), and a Google sign-in whose email already has a
  password account links the provider to that account instead of failing
  with `EMAIL_EXISTS`.
- Likes are anonymous but idempotent per client-generated `viewer_id`.

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
   -> returns job id, stores `GenHandle` in gift record. The exact
   `GenRequest` wire payload is stored too (`gifts.gen_request`, never
   exposed over the API) — the debugging handle when a song comes out wrong:

   ```bash
   ./scripts/show-gen.sh [gift_id]      # no arg = most recent gift
   ./scripts/show-gen.sh | jq -r .music.style   # the composed style string
   ```
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
  meta TEXT,           -- JSON: { name, relationship, style, title, vocal, lang, degraded, ... }
  audio_url TEXT,
  cover_url TEXT,      -- provider cover art (role: Cover asset)
  photos TEXT,         -- JSON array of paths
  gen_handle TEXT,     -- JSON: serialized GenHandle for polling
  gen_status TEXT,     -- 'pending' | 'running' | 'done' | 'failed'
  gen_request TEXT,    -- JSON: exact GenRequest sent to the provider (debug only, never served)
  countdown_status TEXT,
  lrc TEXT,
  duration_secs REAL,
  creator_token TEXT,
  creator_id TEXT,
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
3. **MyGiftsPage** (`/mine`): Every gift of the current creator, from two
   merged sources — the creator tokens kept in localStorage (this device)
   and `GET /api/my-gifts` (the signed-in account, any device). When signed
   in, the device tokens are claimed to the account automatically, so gifts
   created before login follow the account across devices; unpublished ones
   show here even though the playlist never lists them. Status badge
   (generating/ready/failed), publish toggle, and delete live on the card,
   and work from a new device through the session ownership path.
4. **SetPasswordPage** (`/set-password`): set a password for the signed-in
   account — how a passwordless (Google) user adds password sign-in.
5. **ResetPasswordPage** (`/reset-password`): the landing page of the
   password-reset email — redeems the one-time token with a new password,
   signs the user in, and invalidates their old sessions.
6. **PlaylistPage**: Grid of published gifts with play buttons

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
