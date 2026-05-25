# KNA — Claude API 接入文档

> Source: https://wearekna.com/docs
> Retrieved: 2026-05-25
>
> KNA 是 Anthropic API 的代理/网关服务，与 Anthropic 官方 API 100% 兼容。面向中国用户，支持支付宝/微信支付。

## 3 步上手

1. 注册账号 — 邮箱注册，免审核、免海外信用卡
2. 充值任意金额（支付宝 / 微信 / 信用卡均可）
3. Dashboard → API 密钥页生成 `sk-…` Key

## 端点与认证

| 配置项 | 值 |
|--------|-----|
| API Base URL | `https://code.wearekna.com` |
| 认证 Header | `x-api-key: sk-…` |
| SDK 环境变量 | `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` |
| API 版本 | `anthropic-version: 2023-06-01` |

## 可用模型

透传 Anthropic 全部模型。常用 alias：

| Alias | 说明 |
|-------|------|
| `claude-opus-4-7` | 最强能力，适合深度推理 |
| `claude-sonnet-4-6` | 平衡速度与质量（推荐默认） |
| `claude-haiku-4-5` | 最快、最省 tokens，适合辅助任务 |

## Claude Code

Anthropic 官方 CLI。设两个环境变量即可：

```bash
# ~/.zshrc
export ANTHROPIC_BASE_URL="https://code.wearekna.com"
export ANTHROPIC_AUTH_TOKEN="sk-xxxx"
export ANTHROPIC_MODEL="claude-sonnet-4-6"

# 照常使用
claude "重构这个函数"
```

> `ANTHROPIC_AUTH_TOKEN` 用 KNA 的 Key（`sk-` 开头共 67 字符），不是 Anthropic 官方的 `sk-ant-…`。如果之前 `claude /login` 过 OAuth，记得先 `claude /logout` 清掉本地凭据。

## Python SDK

```bash
pip install anthropic
```

```python
from anthropic import Anthropic

client = Anthropic(
    base_url="https://code.wearekna.com",
    api_key="sk-xxxx",
)

msg = client.messages.create(
    model="claude-sonnet-4-6",
    max_tokens=1024,
    messages=[{"role": "user", "content": "你好"}],
)
print(msg.content[0].text)
```

## TypeScript SDK

```bash
npm install @anthropic-ai/sdk
```

```typescript
import Anthropic from "@anthropic-ai/sdk";

const client = new Anthropic({
  baseURL: "https://code.wearekna.com",
  apiKey: "sk-xxxx",
});

const msg = await client.messages.create({
  model: "claude-sonnet-4-6",
  max_tokens: 1024,
  messages: [{ role: "user", content: "你好" }],
});
console.log(msg.content[0].text);
```

## cURL

```bash
curl https://code.wearekna.com/v1/messages \
  -H "x-api-key: sk-xxxx" \
  -H "anthropic-version: 2023-06-01" \
  -H "content-type: application/json" \
  -d '{
    "model": "claude-sonnet-4-6",
    "max_tokens": 1024,
    "messages": [{"role":"user","content":"你好"}]
  }'
```

## Cursor

1. Settings → Models → Anthropic API Key — 填入 `sk-…`
2. 把 `ANTHROPIC_BASE_URL=https://code.wearekna.com` 加到系统环境变量（macOS: `~/.zshrc`，Windows: 系统变量）
3. 重启 Cursor

## Cline / Continue / Aider

这类 VS Code 扩展通常暴露 API Provider 选择和 Custom Base URL 字段：

- Provider：选 Anthropic
- Base URL：`https://code.wearekna.com`
- API Key：`sk-…`
- Model：`claude-sonnet-4-6` 或其他

## 流式响应

设置 `stream: true`，KNA 全程透传 SSE，无额外缓冲延迟。

```python
with client.messages.stream(
    model="claude-sonnet-4-6",
    max_tokens=1024,
    messages=[{"role": "user", "content": "讲个故事"}],
) as stream:
    for text in stream.text_stream:
        print(text, end="", flush=True)
```

## 提示缓存（Prompt Caching）

KNA 完整支持 Anthropic 的 prompt caching，缓存命中按 Anthropic 折扣价计费。

```json
{
  "role": "user",
  "content": [
    {"type": "text", "text": "<long system prompt>", "cache_control": {"type": "ephemeral"}},
    {"type": "text", "text": "实际问题"}
  ]
}
```

## 错误码

| HTTP | 含义 | 处理 |
|------|------|------|
| 401 | API Key 无效或过期 | 检查 Key 拼写，或到 Dashboard 重新生成 |
| 402 | 余额不足 | 充值或检查日额度上限 |
| 429 | 速率超限 | 退避重试，或在套餐设置里提高并发 |
| 500 | KNA 内部错误 | 偶发，重试一次；持续报错联系 info@wearekna.com |
| 503 | Anthropic 上游过载 | 非 KNA 问题，重试或降级到 Haiku |

## 常见问题

**能不能在 Anthropic 官方 Console 用同一个 Key？**
不行。`sk-…` 形式的 KNA Key 只对 KNA 端点有效。

**支持 Tool Use（Function Calling）吗？**
支持。完全透传 Anthropic 的 `tools` 字段，不做改动。

**有 SDK 没有现成示例的语言怎么办？**
用 cURL 或任何 HTTP 客户端，直接 `POST /v1/messages` 即可。

**速率限制是多少？**
余额制账户默认 20 RPM / 200 TPM。高频用户可联系 info@wearekna.com 调大。

## 联系方式

- 邮件：info@wearekna.com
- GitHub Discussions
