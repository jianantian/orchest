# 阿里云百炼 — DashScope Qwen API 参考

> Source: https://help.aliyun.com/zh/model-studio/qwen-api-reference
> Retrieved: 2026-05-25
>
> 通过 DashScope API 调用千问模型（Qwen），支持纯文本、多模态、工具调用、联网搜索、流式输出等功能。

## 地域与端点

### 华北2（北京）

| 模型类型 | HTTP 端点 |
|----------|-----------|
| 纯文本（qwen-plus 等） | `POST https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation` |
| 多模态（qwen3-vl-plus 等） | `POST https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation` |

SDK 调用无需配置 `base_url`。

### 新加坡

| 模型类型 | HTTP 端点 |
|----------|-----------|
| 纯文本 | `POST https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/text-generation/generation` |
| 多模态 | `POST https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation` |

SDK `base_url`：
```python
dashscope.base_http_api_url = 'https://dashscope-intl.aliyuncs.com/api/v1'
```

### 美国（弗吉尼亚）

| 模型类型 | HTTP 端点 |
|----------|-----------|
| 纯文本 | `POST https://dashscope-us.aliyuncs.com/api/v1/services/aigc/text-generation/generation` |
| 多模态 | `POST https://dashscope-us.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation` |

SDK `base_url`：
```python
dashscope.base_http_api_url = 'https://dashscope-us.aliyuncs.com/api/v1'
```

### 德国（法兰克福）

端点格式：`https://{WorkspaceId}.eu-central-1.maas.aliyuncs.com/api/v1/services/aigc/text-generation/generation`

需替换 `{WorkspaceId}` 为真实的 [Workspace ID](https://help.aliyun.com/zh/model-studio/obtain-the-app-id-and-workspace-id)。

## 认证

```
Authorization: Bearer $DASHSCOPE_API_KEY
```

SDK 方式：`api_key=os.getenv('DASHSCOPE_API_KEY')` 或直接传入 `api_key="sk-xxx"`。

> 北京和新加坡地域的 API Key 不同。

---

## 纯文本调用

### 基本请求

```bash
curl -X POST "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation" \
  -H "Authorization: Bearer $DASHSCOPE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen-plus",
    "input": {
      "messages": [
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "你是谁？"}
      ]
    },
    "parameters": {
      "result_format": "message"
    }
  }'
```

```python
import os
import dashscope

messages = [
    {'role': 'system', 'content': 'You are a helpful assistant.'},
    {'role': 'user', 'content': '你是谁？'}
]

response = dashscope.Generation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model="qwen-plus",
    messages=messages,
    result_format='message'
)
print(response)
```

### 流式输出

```bash
curl -X POST "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation" \
  -H "Authorization: Bearer $DASHSCOPE_API_KEY" \
  -H "Content-Type: application/json" \
  -H "X-DashScope-SSE: enable" \
  -d '{
    "model": "qwen-plus",
    "input": {
      "messages": [
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "你是谁？"}
      ]
    },
    "parameters": {
      "result_format": "message",
      "incremental_output": true
    }
  }'
```

> 通过 HTTP 调用流式输出需添加 Header `X-DashScope-SSE: enable`。通过 Java SDK 需使用 `streamCall` 接口。

```python
responses = dashscope.Generation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model="qwen-plus",
    messages=messages,
    result_format='message',
    stream=True,
    incremental_output=True
)
for response in responses:
    print(response)
```

---

## 多模态调用

### 图像理解

```bash
curl -X POST 'https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation' \
  -H "Authorization: Bearer $DASHSCOPE_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "qwen-vl-plus",
    "input": {
      "messages": [{
        "role": "user",
        "content": [
          {"image": "https://example.com/image.jpg"},
          {"text": "这些是什么?"}
        ]
      }]
    }
  }'
```

```python
import dashscope

messages = [{
    "role": "user",
    "content": [
        {"image": "https://dashscope.oss-cn-beijing.aliyuncs.com/images/dog_and_girl.jpeg"},
        {"text": "图中描绘的是什么景象?"}
    ]
}]

response = dashscope.MultiModalConversation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model='qwen-vl-max',
    messages=messages
)
```

图像支持三种传入方式：公网 URL、Base64 编码（`data:image/<format>;base64,<data>`）、本地文件绝对路径。

### 视频理解

传入视频帧（图像列表）或视频文件：

```bash
curl -X POST https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation \
  -H "Authorization: Bearer $DASHSCOPE_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "qwen-vl-max-latest",
    "input": {
      "messages": [{
        "role": "user",
        "content": [
          {"video": ["https://example.com/frame1.jpg", "https://example.com/frame2.jpg"]},
          {"text": "描述这个视频的具体过程"}
        ]
      }]
    }
  }'
```

视频相关参数：
- `fps`：每秒抽帧数，取值范围 [0.1, 10]，默认 2.0
- `max_frames`：抽帧数上限（Qwen3-VL 默认 2000，Qwen3.6/3.5 默认 8000）
- `min_pixels` / `max_pixels`：帧像素阈值
- `total_pixels`：所有帧总像素上限

### 音频理解

```python
messages = [{
    "role": "user",
    "content": [
        {"audio": "https://dashscope.oss-cn-beijing.aliyuncs.com/audios/welcome.mp3"},
        {"text": "这段音频在说什么?"}
    ]
}]

response = dashscope.MultiModalConversation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model='qwen-audio-turbo',
    messages=messages
)
```

---

## 工具调用（Function Calling）

```bash
curl -X POST "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation" \
  -H "Authorization: Bearer $DASHSCOPE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen-plus",
    "input": {
      "messages": [{"role": "user", "content": "杭州天气怎么样"}]
    },
    "parameters": {
      "result_format": "message",
      "tools": [{
        "type": "function",
        "function": {
          "name": "get_current_weather",
          "description": "当你想查询指定城市的天气时非常有用。",
          "parameters": {
            "type": "object",
            "properties": {
              "location": {
                "type": "string",
                "description": "城市或县区，比如北京市、杭州市、余杭区等。"
              }
            },
            "required": ["location"]
          }
        }
      }]
    }
  }'
```

> 使用 `tools` 时必须将 `result_format` 设为 `message`。发起 Function Calling 或提交工具执行结果时都必须设置 `tools` 参数。

```python
tools = [{
    "type": "function",
    "function": {
        "name": "get_current_weather",
        "description": "当你想查询指定城市的天气时非常有用。",
        "parameters": {
            "type": "object",
            "properties": {
                "location": {"type": "string", "description": "城市或县区"}
            },
            "required": ["location"]
        }
    }
}]

response = dashscope.Generation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model='qwen-plus',
    messages=[{"role": "user", "content": "杭州天气怎么样"}],
    tools=tools,
    result_format='message'
)
```

---

## 联网搜索

```python
response = dashscope.Generation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model="qwen-plus",
    messages=[
        {'role': 'system', 'content': 'You are a helpful assistant.'},
        {'role': 'user', 'content': '杭州明天天气是什么？'}
    ],
    enable_search=True,
    result_format='message'
)
```

搜索策略（`search_options.search_strategy`）：
- `turbo`（默认）：兼顾速度与效果
- `max`：更全面的搜索
- `agent`：多轮搜索+整合（支持 qwen3.7-max、qwen3.5-plus 等思考模式）
- `agent_max`：agent 策略 + 网页抓取（支持 qwen3.7-max、qwen3-max 思考模式）

---

## 关键请求参数

| 参数 | 类型 | 必选 | 说明 |
|------|------|------|------|
| `model` | string | 是 | 模型名称（qwen-plus、qwen-max、qwen-vl-max 等） |
| `messages` | array | 是 | 对话消息列表（HTTP 调用时放在 `input` 对象中） |
| `temperature` | float | 否 | 采样温度 [0, 2)，默认 0.7（大多数模型） |
| `top_p` | float | 否 | 核采样阈值 (0, 1.0]，默认 0.8 |
| `top_k` | integer | 否 | 候选集大小，默认 20 |
| `max_tokens` | integer | 否 | 最大输出 Token 数 |
| `seed` | integer | 否 | 随机种子，默认 1234 |
| `stream` | boolean | 否 | 流式输出（仅 Python SDK；HTTP 用 Header） |
| `incremental_output` | boolean | 否 | 流式增量输出，推荐 `true` |
| `result_format` | string | 否 | `text` 或 `message`（推荐 `message`） |
| `enable_thinking` | boolean | 否 | 是否开启思考模式 |
| `thinking_budget` | integer | 否 | 思考过程最大长度 |
| `reasoning_effort` | string | 否 | DeepSeek V4 推理力度：`high` / `max` |
| `tools` | array | 否 | 工具列表（Function Calling） |
| `tool_choice` | string/object | 否 | 工具选择策略：`auto` / `none` / 指定工具 |
| `parallel_tool_calls` | boolean | 否 | 是否开启并行工具调用 |
| `enable_search` | boolean | 否 | 是否开启联网搜索 |
| `search_options` | object | 否 | 联网搜索策略配置 |
| `response_format` | object | 否 | `{"type": "text"}` 或 `{"type": "json_object"}` |
| `stop` | string/array | 否 | 停止词 |
| `repetition_penalty` | float | 否 | 重复惩罚，默认 1.05 |
| `presence_penalty` | float | 否 | 存在惩罚 [-2.0, 2.0]，默认 0 |
| `vl_high_resolution_images` | boolean | 否 | 是否提升图像分辨率上限 |
| `vl_enable_image_hw_output` | boolean | 否 | 是否返回图像缩放后尺寸 |
| `enable_code_interpreter` | boolean | 否 | 是否开启代码解释器 |
| `preserve_thinking` | boolean | 否 | 是否将历史 reasoning_content 拼接至输入 |
| `skill` | array | 否 | 技能参数（PPT 生成，仅 qwen-doc-turbo） |

> HTTP 调用时，除 `model` 和 `input.messages` 外的参数放在 `parameters` 对象中。

### Messages 结构

**System Message**：`{"role": "system", "content": "..."}`

**User Message**：
```json
{
  "role": "user",
  "content": "纯文本" 
}
// 或多模态：
{
  "role": "user",
  "content": [
    {"text": "描述此图"},
    {"image": "https://..."},
    {"audio": "https://..."},
    {"video": ["https://frame1.jpg", ...]}
  ]
}
```

用户消息 content 支持：
- `text`：纯文本
- `image`：图片 URL / Base64 / 本地路径
- `video`：视频帧列表或视频文件 URL
- `audio`：音频文件 URL
- `cache_control`：显式缓存控制 `{"type": "ephemeral"}`

**Assistant Message**：`{"role": "assistant", "content": "..."}`，支持 `tool_calls` 和 `partial`（前缀续写）

**Tool Message**：`{"role": "tool", "content": "...", "tool_call_id": "..."}`

---

## 响应格式

### 非流式响应

```json
{
  "status_code": 200,
  "request_id": "902fee3b-f7f0-9a8c-96a1-6b4ea25af114",
  "code": "",
  "message": "",
  "output": {
    "text": null,
    "finish_reason": null,
    "choices": [{
      "finish_reason": "stop",
      "message": {
        "role": "assistant",
        "content": "我是阿里云开发的一款超大规模语言模型，我叫千问。"
      }
    }]
  },
  "usage": {
    "input_tokens": 22,
    "output_tokens": 17,
    "total_tokens": 39
  }
}
```

### 响应字段

| 字段 | 说明 |
|------|------|
| `status_code` | 200 表示成功 |
| `request_id` | 唯一标识符 |
| `output.choices[].message.content` | 模型回复文本 |
| `output.choices[].message.reasoning_content` | 深度思考内容 |
| `output.choices[].message.tool_calls` | 工具调用（含 `function.name`、`function.arguments`） |
| `output.choices[].finish_reason` | `stop` / `length` / `tool_calls` / null |
| `output.choices[].logprobs` | Token 概率信息 |
| `output.choices[].search_info` | 联网搜索结果 |
| `usage.input_tokens` | 输入 Token 数 |
| `usage.output_tokens` | 输出 Token 数 |
| `usage.total_tokens` | 总 Token 数 |
| `usage.output_tokens_details.reasoning_tokens` | 思考 Token 数 |
| `usage.prompt_tokens_details.cached_tokens` | 缓存命中 Token 数 |

### finish_reason 值

| 值 | 含义 |
|----|------|
| `null` | 正在生成中 |
| `stop` | 自然结束或触发 stop 条件 |
| `length` | 因长度限制提前终止 |
| `tool_calls` | 因需要工具调用而终止 |

---

## 特殊功能

### PPT 生成（仅 qwen-doc-turbo）

```python
response = dashscope.Generation.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model='qwen-doc-turbo',
    messages=[
        {"role": "system", "content": "you are a helpful assistant."},
        {"role": "system", "content": "文档内容"},
        {"role": "user", "content": "生成一个10到20页的ppt"}
    ],
    skill=[{"type": "ppt", "mode": "general", "template_id": "news_01"}]
)
```

### 文档理解（qwen-long）

通过 `fileid://{FILE_ID}` 在 system message 中引用已上传的文件：

```python
messages = [
    {'role': 'system', 'content': 'you are a helpful assistant'},
    {'role': 'system', 'content': 'fileid://{FILE_ID}'},
    {'role': 'user', 'content': '这篇文章讲了什么'}
]
```

### 异步调用

```python
from dashscope.aigc.generation import AioGeneration

response = await AioGeneration.call(
    api_key=os.getenv('DASHSCOPE_API_KEY'),
    model="qwen-plus",
    messages=[{"role": "user", "content": "你是谁"}],
    result_format="message",
)
```

---

## 支持的模型系列

| 系列 | 示例模型 |
|------|----------|
| Qwen 商业版 | qwen-plus, qwen-max, qwen-turbo, qwen-long |
| Qwen 开源版 | qwen3-8b, qwen3-32b, qwen2.5-72b |
| Qwen-VL | qwen-vl-max, qwen-vl-plus, qwen3-vl-plus |
| Qwen-Coder | qwen-coder-plus |
| Qwen-Audio | qwen-audio-turbo |
| QwQ / QVQ | qwq-32b, qvq-max |
| DeepSeek（阿里云直供） | deepseek-v4-pro, deepseek-v4-flash, deepseek-r1 |
| Kimi（阿里云直供） | kimi-k2.6, kimi-k2.5 |
| GLM（阿里云直供） | glm-5.1, glm-5, glm-4.7 |
| MiniMax（阿里云直供） | MiniMax-M2.5, MiniMax-M2.1 |
