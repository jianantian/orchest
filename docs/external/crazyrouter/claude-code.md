# Crazyrouter — Claude Code Setup Guide

> Source: https://docs.crazyrouter.com/en/integrations/claude-code
> Retrieved: 2026-05-25

## Overview

Claude Code speaks the Anthropic Messages API directly. With a few environment variables, it can send Anthropic requests to Crazyrouter.

| Setting | Value |
|---------|-------|
| Protocol | Anthropic Messages API |
| Base URL | `https://crazyrouter.com` (root, no `/v1`) |
| China-optimized | `https://cn.crazyrouter.com` |
| Auth variable | `ANTHROPIC_API_KEY` |
| Recommended model | `claude-sonnet-4-6` |

> Do not append `/v1` or `/v1/messages`. Claude Code appends the request path itself.

## Quick Setup

```bash
# ~/.zshrc or ~/.bashrc
export ANTHROPIC_BASE_URL=https://crazyrouter.com
export ANTHROPIC_API_KEY=sk-xxx

# Optional: set default model
export ANTHROPIC_MODEL=claude-sonnet-4-6

# Launch
claude
```

For China-optimized route:
```bash
export ANTHROPIC_BASE_URL=https://cn.crazyrouter.com
```

## Prerequisites

| Item | Notes |
|------|-------|
| Crazyrouter account | https://crazyrouter.com |
| Crazyrouter token | Dedicated `sk-...` token for Claude Code |
| Git | 2.23+ recommended |
| Node.js | 18+ |
| Claude Code | `npm install -g @anthropic-ai/claude-code` |
| Model allowlist | At least `claude-sonnet-4-6`, `claude-opus-4-6` |

## Using Non-Claude Models in Claude Code

Crazyrouter exposes third-party models through the Anthropic Messages protocol. Switch via `ANTHROPIC_MODEL` or `/model` command.

### Verified Working Models

| Model ID | Provider | Tool Use | Streaming | Thinking | Best For |
|----------|----------|----------|-----------|----------|----------|
| `deepseek-v4-pro` | DeepSeek | ✅ | ✅ | — | General coding, best price/perf |
| `deepseek-v4-flash` | DeepSeek | ✅ | ✅ | ✅ native | Reasoning with visible thinking |
| `deepseek-reasoner` | DeepSeek | ✅ | ✅ | reasoning | Math / complex logic |
| `deepseek-chat` / `deepseek-v3` | DeepSeek | ✅ | ✅ | — | General text |
| `MiniMax-M2.7` | MiniMax | ✅ | ✅ | ⚠️ `<think>` tags | Long-context CN |
| `MiniMax-M2.5` | MiniMax | ✅ | ✅ | ⚠️ `<think>` tags | General chat |
| `MiniMax-M2.1` | MiniMax | ✅ | ✅ | — | Clean output |
| `kimi-k2.5` / `kimi-k2` | Moonshot Kimi | ✅ | ✅ | — | Long-context CN |
| `kimi-k2-thinking` | Moonshot Kimi | ✅ | ✅ | reasoning | Complex reasoning |

### Model Switching

```bash
# Environment variable
export ANTHROPIC_MODEL=deepseek-v4-pro

# One-off
ANTHROPIC_MODEL=kimi-k2-thinking claude

# In-session
/model deepseek-v4-pro
```

### Quick Validation

```bash
curl -sS https://crazyrouter.com/v1/messages \
  -H "Authorization: Bearer $ANTHROPIC_API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-v4-pro",
    "max_tokens": 32,
    "messages": [{"role":"user","content":"reply OK"}]
  }'
```

A `200` with non-empty `content[].text` means it works.

### Usage Notes

- **DeepSeek family**: highest protocol fidelity. `deepseek-v4-flash` returns native `thinking` blocks. **Recommended default: `deepseek-v4-pro`**.
- **Kimi family**: strong long-context. `kimi-k2-thinking` includes reasoning trace.
- **MiniMax family**: `M2.5`/`M2.7` emit reasoning as `<think>...</think>` text blocks. Use `MiniMax-M2.1` to avoid this, or add a system prompt: "Do not output `<think>` tags."
- Token allowlist must include any non-Claude model used.

### Unsupported Models

The following return `get_channel_failed` on Anthropic protocol. Use `/v1/chat/completions` (OpenAI protocol) instead:
- `moonshot-v1-8k` / `moonshot-v1-32k` / `moonshot-v1-128k`
- Channels: `grok-*`, `coze`, `jimeng`, `baidu`, `zhipu`, `tencent`, `xunfei`, `mistral`, `cohere`, `palm`

## Token Best Practices

| Setting | Recommendation |
|---------|---------------|
| Dedicated token | Do not share with Cursor, Codex, or OpenClaw |
| Model allowlist | Enable (usually 1-2 models needed) |
| IP restriction | Recommended on fixed-egress |
| Quota cap | Strongly recommended (tool use = steady consumption) |

## Common Errors

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `401` | Invalid/expired `ANTHROPIC_API_KEY` | Create new token |
| `403` / `model not allowed` | Token doesn't allow the model | Add model to allowlist |
| `404` | Base URL has `/v1` or `/v1/messages` appended | Use root domain only |
| `/v1/v1/messages` in logs | Base URL already contains path | Remove path from base URL |
| Old settings persist | Env vars not reloaded | `source ~/.zshrc` or reopen terminal |
