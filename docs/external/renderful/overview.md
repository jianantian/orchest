# Renderful — API Documentation

> Source: https://renderful.ai/zh/docs
> Retrieved: 2026-05-25
>
> Renderful is a unified AI generation API gateway. One API key covers image, video, audio, 3D, and text generation from 30+ providers.

## Quick Start

### 1. Get Your API Key

Create from Dashboard → API Keys:

```bash
export RENDERFUL_API_KEY="rf_your_key_here"
```

### 2. Submit a Task

```bash
curl -X POST https://api.renderful.ai/api/v1/generations \
  -H "Authorization: Bearer $RENDERFUL_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "type": "text-to-image",
    "model": "flux-dev",
    "prompt": "A beautiful sunset over mountains"
  }'
```

> `type` is required. If omitted, the API returns `400 Bad Request`.

### 3. Poll for Result

Generation is async. Poll `GET /api/v1/generations/:id` until `completed` or `failed`.

```bash
while true; do
  RESULT=$(curl -s -H "Authorization: Bearer $RENDERFUL_API_KEY" \
    https://api.renderful.ai/api/v1/generations/gen_abc123)
  STATUS=$(echo $RESULT | grep -o '"status":"[^"]*"' | head -1 | cut -d'"' -f4)
  echo "Status: $STATUS"
  [ "$STATUS" = "completed" ] || [ "$STATUS" = "failed" ] && break
  sleep 3
done
```

## API Reference

| Item | Value |
|------|-------|
| Base URL | `https://api.renderful.ai/api/v1` |
| Auth Header | `Authorization: Bearer YOUR_API_KEY` |
| Content-Type | `application/json` |
| Create task | `POST /api/v1/generations` |
| Get result | `GET /api/v1/generations/:id` |
| Developer balance | `GET /api/v1/account/balance` |
| Agent balance | `GET /api/v1/agents/balance` |

## Status Values

| Status | Description |
|--------|-------------|
| `queued` | Task received, waiting for a worker |
| `processing` | Generation in progress |
| `completed` | Done — `outputs` array contains result URLs |
| `failed` | Error — check `error` field for details |

## Supported `type` Values

```
text-to-image      image-to-image     text-to-video
image-to-video     video-to-video     reference-to-video
subject-to-video   upscale            face-swap
lip-sync           text-to-music      text-to-audio
audio-to-audio     speech-to-text     text-to-3d
image-to-3d        text-to-text
```

List available models: `GET /api/v1/models?type=text-to-image`

## LLM Quick Start (`text-to-text`)

LLM calls use the same endpoint with `type: "text-to-text"`:

```bash
curl -X POST https://api.renderful.ai/api/v1/generations \
  -H "Authorization: Bearer $RENDERFUL_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "type": "text-to-text",
    "model": "kimi-k2.5",
    "prompt": "Summarize this article in 5 bullets",
    "system_prompt": "You are a concise assistant.",
    "max_tokens": 512,
    "temperature": 0.7
  }'
```

> OpenAI-style `messages` array is also supported. Use `/api/v1/generations` (not `/v1/chat/completions`).
