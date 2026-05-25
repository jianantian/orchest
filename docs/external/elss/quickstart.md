# Elss — Quickstart Guide

> Source: https://elss.ai/docs/quickstart
> Retrieved: 2026-05-25

Most teams only need to change the base URL and API key.

## Step 1: Get Your API Key

1. Sign up at https://elss.ai/register
2. Dashboard → API Keys → Create New Key
3. Copy your key (starts with `sk-`)

## Step 2: Make Your First Request

### cURL

```bash
curl https://api.elss.ai/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-your-key" \
  -d '{
    "model": "gpt-4.1",
    "messages": [
      {"role": "user", "content": "Hello! What models do you support?"}
    ]
  }'
```

### Python (OpenAI SDK)

```bash
pip install openai
```

```python
from openai import OpenAI

client = OpenAI(
    base_url="https://api.elss.ai/v1",
    api_key="sk-your-key",
)

response = client.chat.completions.create(
    model="gpt-4.1",
    messages=[
        {"role": "user", "content": "Hello! What models do you support?"}
    ],
)

print(response.choices[0].message.content)
```

### TypeScript (OpenAI SDK)

```bash
npm install openai
```

```typescript
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "https://api.elss.ai/v1",
  apiKey: "sk-your-key",
});

const response = await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [
    { role: "user", content: "Hello! What models do you support?" },
  ],
});

console.log(response.choices[0].message.content);
```

## Step 3: Streaming Responses

```python
from openai import OpenAI

client = OpenAI(
    base_url="https://api.elss.ai/v1",
    api_key="sk-your-key",
)

stream = client.chat.completions.create(
    model="claude-sonnet-4-20250514",
    messages=[{"role": "user", "content": "Write a short poem about APIs"}],
    stream=True,
)

for chunk in stream:
    if chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="")
```

## Step 4: List Available Models

```bash
curl https://api.elss.ai/v1/models \
  -H "Authorization: Bearer sk-your-key"
```

## What's Next

- [API Reference](/docs/external/elss/api-reference.md) — Full endpoint documentation
- [Claude Code Setup](https://elss.ai/tutorials/claude-code) — Use with Claude Code
- [Cursor Setup](https://elss.ai/tutorials/cursor) — Use with Cursor IDE
