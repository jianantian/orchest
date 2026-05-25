# Elss — API Reference

> Source: https://elss.ai/docs/api-reference
> Retrieved: 2026-05-25
>
> Elss is an API gateway/aggregator that exposes a unified API key for OpenAI, Anthropic, and other model providers, with transparent pricing and built-in failover.

## Base URL

| Purpose | URL |
|---------|-----|
| OpenAI-compatible SDK base URL | `https://api.elss.ai/v1` |
| HTTP API origin | `https://api.elss.ai` |

With the SDK base URL, `POST /v1/chat/completions` is called as `/chat/completions` from OpenAI SDK clients.

## Authentication

```
Authorization: Bearer sk-your-key
```

- Anthropic-compatible clients may also send `X-Api-Key: sk-your-key`.
- Never expose API keys in browser code, mobile apps, public repositories, or client-side logs.

## Request Contract

- `Content-Type: application/json` for JSON requests.
- `stream: true` on supported generation endpoints for Server-Sent Events.
- `GET /v1/models` for model discovery.
- Provider-specific features depend on the selected model and channel capability; unsupported options return a structured error.

---

## Endpoints

### Chat & Text Completions

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/chat/completions` | Chat completions — primary endpoint |
| POST | `/v1/completions` | Legacy text completions |
| POST | `/v1/edits` | Legacy edits endpoint |

#### POST /v1/chat/completions

**Required:** `model` (string), `messages` (array of `{role, content}`)

**Common optional:** `max_tokens` / `max_completion_tokens`, `temperature`, `top_p`, `top_k`, `stop`, `stream`, `stream_options.include_usage`, `tools`, `tool_choice`, `parallel_tool_calls`, `response_format`, `reasoning_effort`, `verbosity`, `metadata`, `user`, `service_tier`, `seed`

**Response:** `choices[].message`, `choices[].finish_reason`, `model`, `created`, `usage`. Streaming uses SSE with `choices[].delta`.

### Responses API

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/responses` | Create a response |
| POST | `/v1/responses/compact` | Compact context (native Responses only) |
| POST | `/v1/responses/input_tokens` | Count input tokens (native Responses only) |
| GET | `/v1/responses/:id` | Retrieve a response |
| GET | `/v1/responses/:id/input_items` | List input items (native Responses only) |
| DELETE | `/v1/responses/:id` | Delete a response |
| POST | `/v1/responses/:id/cancel` | Cancel in-progress response |

#### POST /v1/responses

**Required:** `model` (string), `input` or `prompt`

**Common optional:** `instructions`, `previous_response_id`, `store`, `background`, `max_output_tokens`, `temperature`, `top_p`, `tools`, `tool_choice`, `parallel_tool_calls`, `max_tool_calls`, `reasoning`, `text`, `include`, `truncation`, `stream`, `metadata`, `user`, `service_tier`, `safety_identifier`

**Notes:** Retrieve, input_items, input_tokens, cancel, delete, and compact actions require native Responses API upstream support.

### Claude Messages (Anthropic Native)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/messages` | Anthropic-compatible Messages endpoint |

#### POST /v1/messages

**Required:** `model` (string), `max_tokens` (number), `messages` (array of `{role, content}`)

**Common optional:** `system`, `temperature`, `top_p`, `top_k`, `stream`, `stop_sequences`, `tools`, `tool_choice`, `thinking`, `reasoning_effort`, `effort`, `output_config`, `metadata`

**Response:** JSON uses `id`, `type`, `role`, `model`, `content[]`, `stop_reason`, `stop_sequence`, `usage`. Streaming uses Anthropic Messages SSE events.

### Embeddings

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/embeddings` | Create text embeddings |
| POST | `/v1/engines/:model/embeddings` | Legacy path (OpenAI v1 compat) |

**Required:** `model` (string), `input` (string or array)

**Common optional:** `encoding_format`, `dimensions`, `user`

### Rerank

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/rerank` | Rerank documents by relevance |
| POST | `/v2/rerank` | V2 endpoint (Cohere-compatible) |

**Required:** `model` (string), `query` (string), `documents` (string[])

**Common optional:** `top_n`, `max_tokens_per_doc`, `priority`, `input` (legacy query alias)

### Images

Requires a channel with image generation capability.

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/images/generations` | Generate images from text |
| POST | `/v1/images/edits` | Edit an existing image |

**Required:** `generations`: `prompt`; `edits`: `image`, `mask`, `model`, `prompt`

**Common optional:** `model`, `n`, `size`, `quality`, `style`, `response_format`, `aspect_ratio`, `output_format`, `background`, `moderation`, `user`, `image_prompt`, `input_fidelity`

Image edits require multipart form upload.

### Audio

Requires a channel with audio capability.

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/audio/speech` | Text-to-speech (TTS) |
| POST | `/v1/audio/transcriptions` | Audio to text (Whisper) |
| POST | `/v1/audio/translations` | Transcribe and translate to English |

**Required:** `speech`: `model`, `input`, `voice`; `transcriptions/translations`: `file`, `model`

**Common optional:** `speech`: `speed`, `response_format`; `transcriptions/translations`: `prompt`, `response_format`, `temperature`; `transcriptions`: `language`, `timestamp_granularity`

Transcription/translation use multipart form upload.

### Video

Requires a channel with video generation capability.

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/videos` | Submit a video task |
| GET | `/v1/videos` | List video tasks |
| GET | `/v1/videos/:id` | Get task status |
| GET | `/v1/videos/:id/content` | Download completed video |
| DELETE | `/v1/videos/:id` | Delete a video task |
| POST | `/v1/video/generations` | Legacy video generation submit path |
| GET | `/v1/video/generations/:id` | Legacy video generation status path |

Video tasks are asynchronous on supported video channels.

**Required:** `model` and/or `prompt`, depending on channel

**Common optional:** `seconds`, `duration`, `duration_seconds`, `size`, `resolution`, `aspect_ratio`, `remix_id`, `reference_id`, `reference_assets`, `generate_audio`, `seed`, `return_last_frame`, `callback_url`

### Moderation

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/moderations` | Classify text for policy violations |

**Required:** `model` (string), `input` (string or array)

### Realtime

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/realtime` | WebSocket for real-time sessions |
| GET | `/v1/responses` | WebSocket upgrade for native Responses sessions |

### MCP (Model Context Protocol)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/mcp` | MCP proxy — route tool calls |

MCP responses follow the selected upstream capability.

### Asset Management (Volcengine)

Requires a channel with asset management capability. All endpoints use POST with JSON body.

| Method | Path | Description |
|--------|------|-------------|
| POST | `/volc/asset/CreateAssetGroup` | Create an asset group |
| POST | `/volc/asset/CreateAsset` | Upload a reference asset to a group |
| POST | `/volc/asset/ListAssetGroups` | List your asset groups |
| POST | `/volc/asset/ListAssets` | List assets in a group |
| POST | `/volc/asset/GetAssetGroup` | Get asset group details |
| POST | `/volc/asset/GetAsset` | Get a single asset |
| POST | `/volc/asset/UpdateAssetGroup` | Rename or update an asset group |
| POST | `/volc/asset/UpdateAsset` | Update asset metadata |

### Models

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/models` | List all available models |
| GET | `/v1/models/:model` | Retrieve model details |

The model list is dynamic and filtered by API key, user entitlement, channel status, and configured model mappings.

### Special Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/paas/v4/layout_parsing` | Zhipu OCR / document parsing |

---

## Streaming

Supported on compatible text, Responses, Claude Messages, and media routes when the upstream channel supports the mode.

- Chat Completions and Responses: `stream: true` → SSE.
- Claude Messages: Anthropic-shaped SSE events.
- Realtime: WebSocket flows.

If a stream has already started and an error occurs, the gateway sends a protocol-appropriate SSE error event instead of corrupting the stream with a JSON body.

## Special Compatibility

### Claude Code Path Normalization

Claude Code may send Messages API requests through different path prefixes. Elss rewrites all of the following to `POST /v1/messages`:

- `/openai/v1/messages`
- `/v1/v1/messages`
- `/openai/v1/v1/messages`
- `/api/v1/v1/messages`

No client-side configuration required.

### API Format Auto-Detection

If a request body is sent to the wrong endpoint (e.g., Responses API payload to `/v1/chat/completions`), Elss detects the mismatch and routes it to the correct handler by default.

---

## Error Codes

Relay endpoints return sanitized public error messages:

| Code | Meaning |
|------|---------|
| 400 | Invalid request, unsupported parameter, unsupported channel capability, or model mismatch |
| 401 | Authentication failed. Check the API key and header format |
| 403 | Permission denied or account balance is insufficient |
| 404 | The requested resource or route was not found |
| 408/504 | The request timed out |
| 413 | The request body or model input is too large |
| 429 | Rate limit, concurrency limit, or service busy condition |
| 500/502/503 | Temporary service or upstream provider failure |

**Error shape (OpenAI-compatible):**
```json
{
  "error": {
    "message": "The request is too long for the selected model.",
    "type": "invalid_request_error",
    "param": "",
    "code": "context_window_exceeded"
  }
}
```

Claude Messages JSON errors use the Anthropic `{ type: "error", error: { type, message } }` envelope.

**Retry guidance:**
- Retry 408, 429, 500, 502, 503, 504 with exponential backoff and jitter.
- Do not retry 400, 401, 403, 404, 413 until the request, credentials, permissions, balance, or input size is corrected.

---

## Unsupported

The following OpenAI-compatible route families are registered for clear 501 responses but are not supported:

- `/v1/files`
- `/v1/fine_tuning/jobs`
- `/v1/assistants`
- `/v1/threads`
- `/v1/images/variations`
- `DELETE /v1/models/:model`
