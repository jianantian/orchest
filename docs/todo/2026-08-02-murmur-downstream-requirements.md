# TODO: Python SDK 下游需求（来自 Murmur）

> 状态: **待评估** | 记录于 2026-08-02
> 来源: Murmur（念念，`~/Develop/murmur`）—— Python FastAPI 后端，尝试作为 orchest Python SDK
> 的第一个真实下游消费者。以下四条是接入过程中撞到的能力缺口，按阻塞程度排序。

## 背景

Murmur 服务端的 AI 用量非常"朴素"，与 agent 场景不同：

- **单次 JSON completion**（非对话、非工具调用）：fragment 分析（标题/标签/分类/线程归属）、
  thread 摘要、每日/每周总结。每次调用 = system prompt + user prompt → 一个 JSON。
- **非实时语音文件转写**：移动端录 m4a（≤5 分钟 / ≤10MB）→ HTTP 上传到服务端 → 转写成文本。

LLM 侧计划用 `deepseek/deepseek-chat`（provider 已存在，✅ 已满足）；
ASR 侧计划用阿里云 DashScope 非实时模型（如 `qwen3-asr-flash`，OpenAI 兼容 HTTP，支持
base64 音频输入）。

## 需求 1 — Python SDK 暴露轻量 completion API（阻塞中，有 workaround）

**现状**：Python 绑定（`crates/orchest-py`）只暴露 `Agent`（完整 agent 循环：
`run` / `run_sync` / `run_stream` + 事件流）。做一次单次 completion 需要构造 Agent、
跑完整 loop、从 `run_completed` 事件里捞 output——对非 agent 场景过重，且语义上
（budget、steps、tools）都不匹配。

**期望**：暴露 `ModelAdapter` 层的直接调用，例如：

```python
from orchest import complete

text = complete(
    model="deepseek/deepseek-chat",
    system="...",
    user="...",
    api_key_env="DEEPSEEK_API_KEY",
    json_mode=True,   # 可选：response_format json_object
    retry=True,       # 复用现有 429/5xx/timeout 重试策略
)
```

**验收标准**：Python 侧不超过 5 行完成一次带 system prompt 的单次 completion，
不经过 agent loop。

## 需求 2 — ASR 能力暴露到 Python，且支持非实时文件转写（完全阻塞）

**现状**：

- ASR 实现只存在于 `orchest-provider-stream`（Rust），且只有**流式 WebSocket** 方言
  （`fun-asr-realtime` 等，run-task/finish-task 双工）。
- Python 绑定**完全没有暴露 ASR**。
- Rust 侧的 `Asr::transcribe`（非流式）在 aliyun provider 里直接返回
  `UnsupportedOperation`（"streaming-only"）。

Murmur 的场景是**录完再转**，实时双工对它是过度设计（还引入 PCM 重采样、WS 会话管理等
一系列本不存在的问题）。

**期望**：

1. Rust 侧补一个 HTTP（非 WS）ASR 方言：DashScope 非实时模型，默认一个——
   **`qwen-audio-3.0-asr-flash`**（同步，≤5 分钟短音频，multimodal-generation 端点，
   支持 base64 直传）。选型说明：`fun-asr`（0.00022 元/秒）单价更低，但为异步任务
   且仅收公网文件 URL，不满足"字节直传、同步返回"的接入形态，仅作备选。
2. Python 暴露：

```python
from orchest import transcribe

text = transcribe(
    audio_bytes,
    format="m4a",
    language="zh",
    provider="aliyun/qwen-audio-3.0-asr-flash",
    api_key_env="DASHSCOPE_API_KEY",
)
```

**验收标准**：Python 传入音频字节得到转写文本；覆盖 m4a 输入（移动端录音格式）。

## 需求 3 — Python SDK 分发方式（接入摩擦）

**现状**：`orchest-py` 未发布到任何 index；预编译 `.so` 绑定特定 cpython 版本
（仓库内为 cp314，Murmur venv 为 3.12，直接不可用）。下游目前只能 clone 仓库 +
本地 maturin 构建，且 `uv` path-dependency 接法未文档化。

**期望**（任一即可）：

- 发布 wheel 到内部/公开 index；或
- 在 `docs/guide/` 写明下游项目用 uv 以 path/source 依赖接入的标准做法
  （含目标 Python 版本不一致时的重建步骤）。

## 需求 4 — Agent 使用显式名称（日志与 handoff 可读性）

**现状**：`AgentConfig` 没有 agent identity 字段；`on_run_start` 与 handoff 的 hook / event
从 `system_prompt` 截取前 60 个字符作为临时名称。名称因此可能是半截中文 prompt，且 prompt
前缀相同的 agent 无法区分。

**期望**：Rust `AgentConfig`、Python `Agent(...)`、Node `AgentOptions` 都要求显式提供
`name`。run start 与 handoff 的日志、hook、event 直接使用该字段，不从 `system_prompt`
生成或 fallback；prompt 内容仍原样发送给模型。

**验收标准**：`on_run_start` 能读取当前 agent 名称；handoff 能准确记录
`previous_agent → new_agent`，且两者与 system prompt 文案相互独立。

## 需求 2 补充 — 流式中继是已排期的后续方向（2026-08-02 追加）

Murmur 已确认两条 ASR 路线都保留：

- **非实时文件转写 = 当前刚需**（MVP，见需求 2 正文）。
- **流式中继 = v1.1 体验升级（已排期）**：端上边录边推音频 chunk → Murmur 服务端
  WebSocket 中继 → DashScope 实时模型，文字边录边回显。
  鉴权在 WS 握手，key 不出服务端。实时模型默认一个：
  **`aliyun/qwen-audio-3.0-asr-flash-streaming`**（orchest catalog 已有）。
  选型说明：与 `fun-asr-realtime` 同价（均 0.00033 元/秒、免费额度 36,000 秒）、
  同一 inference WS 方言可互换；胜出原因是支持 **Context 上下文增强**——
  Murmur 可把用户近期的 entities/人名按请求动态注入，专有名词识别更准，
  且与非实时默认 `qwen-audio-3.0-asr-flash` 同家族，口径统一。
  `fun-asr-realtime`（热词表、方言）留作同方言备选。届时若 orchest 的流式 ASR（
  `orchest-provider-stream` 现有方言）已暴露到 Python，Murmur 会直接采用，
  不再自建中继。因此需求 2 的"暴露 ASR 到 Python"请把流式方言一并考虑在内。

## 备注

- 在以上能力落地前，Murmur 的临时方案：LLM 用 `Agent.run_sync` 包一层
  `complete_json`（需求 1 的 workaround）；ASR 在 Python 侧直接调 DashScope
  multimodal-generation 端点（需求 2 无法 workaround，只能绕过 SDK）。
- 若 orchest 侧认为"轻量 completion / 文件转写"不属于 SDK 范围（minimal core 原则），
  也请明确告知，Murmur 将改为完全直连 provider API，不再依赖 SDK。
