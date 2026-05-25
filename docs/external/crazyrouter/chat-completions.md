# Crazyrouter — Chat Completions API

> Source: https://docs.crazyrouter.com/en/chat/openai/completions
> Retrieved: 2026-05-25

## Endpoint

```
POST /v1/chat/completions
```

Generates model responses from a message list. Supports non-streaming and streaming output.

## Authentication

```
Authorization: Bearer YOUR_API_KEY
```

## Core Parameters

| Parameter | Type | Required | Meaning |
|-----------|------|----------|---------|
| `model` | string | Yes | e.g. `gpt-5.4`, `claude-sonnet-4-6`, `gemini-3-pro` |
| `messages` | array | Yes | Message list `[{role, content}]` |
| `stream` | boolean | No | Enable SSE streaming |
| `max_tokens` | integer | No | Maximum output tokens |
| `temperature` | number | No | Sampling temperature |
| `response_format` | object | No | Structured output constraint |
| `tools` | array | No | Tool list |
| `tool_choice` | string\|object | No | Tool-selection strategy |
| `stream_options` | object | No | e.g. `include_usage` |

> Optional parameter support varies by model. Rely on common fields first.

## Non-Streaming Request

```bash
curl https://crazyrouter.com/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-5.4",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "Explain AI in one sentence."}
    ],
    "max_tokens": 64
  }'
```

Response shape:
```json
{
  "object": "chat.completion",
  "model": "gpt-5.4",
  "choices": [{
    "message": {
      "role": "assistant",
      "content": "...",
      "reasoning_content": null,
      "tool_calls": null
    },
    "finish_reason": "stop"
  }]
}
```

- `message.content` — stable final-text field
- `message.tool_calls` — only when model requests tool execution
- `message.reasoning_content` — may exist as key, but not guaranteed usable

## Streaming Request

```bash
curl https://crazyrouter.com/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-5.4",
    "messages": [{"role": "user", "content": "Explain AI in one sentence."}],
    "stream": true,
    "stream_options": {"include_usage": true},
    "max_tokens": 64
  }'
```

- Returns `chat.completion.chunk`
- SSE chunks: `data: {"object":"chat.completion.chunk",...}`
- Stream ends with `data: [DONE]`

## Python Example (Streaming)

```python
from openai import OpenAI

client = OpenAI(
    api_key="YOUR_API_KEY",
    base_url="https://crazyrouter.com/v1"
)

stream = client.chat.completions.create(
    model="gpt-5.4",
    messages=[{"role": "user", "content": "Explain AI in one sentence."}],
    stream=True,
    stream_options={"include_usage": True},
    max_tokens=64
)

for chunk in stream:
    delta = chunk.choices[0].delta
    if delta.content is not None:
        print(delta.content, end="")
```

## Recommendations

- Normal text chat: `/v1/chat/completions`
- Reasoning summaries or OpenAI-style web search: `/v1/responses`
- For specific capability guarantees, use dedicated capability pages
