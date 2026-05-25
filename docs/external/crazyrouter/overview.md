# Crazyrouter — Overview & Quick Start

> Source: https://docs.crazyrouter.com
> Retrieved: 2026-05-25
>
> Crazyrouter is a unified AI model API gateway. Use one API key for text, image, video, audio, embeddings, rerank, and the model access needed by common AI tools.

## API Endpoints

| Use Case | Base URL |
|----------|----------|
| OpenAI-compatible clients | `https://crazyrouter.com/v1` |
| OpenAI-compatible (China-optimized) | `https://cn.crazyrouter.com/v1` |
| Claude Code / Anthropic-native clients | `https://crazyrouter.com` (root, no `/v1`) |
| Claude Code (China-optimized) | `https://cn.crazyrouter.com` (root, no `/v1`) |

> Claude Code appends the Anthropic request path itself. Do not append `/v1` or `/v1/messages` to the base URL for Anthropic-native clients.

## Quick Start

### Step 1: Get Your API Key

1. Visit https://crazyrouter.com and create an account
2. Go to Token Management → Create Token

### Step 2: Make a Request

```bash
curl https://crazyrouter.com/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-5.4",
    "messages": [
      {"role": "user", "content": "Hello"}
    ]
  }'
```

```python
from openai import OpenAI

client = OpenAI(
    api_key="YOUR_API_KEY",
    base_url="https://crazyrouter.com/v1"
)

response = client.chat.completions.create(
    model="gpt-5.4",
    messages=[{"role": "user", "content": "Hello"}]
)
print(response.choices[0].message.content)
```

```typescript
import OpenAI from 'openai';

const client = new OpenAI({
  apiKey: 'YOUR_API_KEY',
  baseURL: 'https://crazyrouter.com/v1'
});

const response = await client.chat.completions.create({
  model: 'gpt-5.4',
  messages: [{ role: 'user', content: 'Hello' }]
});
console.log(response.choices[0].message.content);
```

### Step 3: Response

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "choices": [{
    "index": 0,
    "message": { "role": "assistant", "content": "Hello! How can I help you today?" },
    "finish_reason": "stop"
  }],
  "usage": { "prompt_tokens": 9, "completion_tokens": 12, "total_tokens": 21 }
}
```

## Common Paths

| Goal | Path |
|------|------|
| Base URLs and regional routes | See [API Endpoints](#api-endpoints) above |
| Authentication | `Authorization: Bearer YOUR_API_KEY` |
| Connect Claude Code | See [Claude Code Setup](claude-code.md) |
| Send OpenAI-compatible request | `POST /v1/chat/completions` |
| Image generation | `POST /v1/images/generations` |
| Video generation | `POST /v1/video/create` |

## Documentation Index

Full index at: https://docs.crazyrouter.com/llms.txt
