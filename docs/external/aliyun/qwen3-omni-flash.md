# Qwen3-Omni-Flash（阿里云百炼）

> 全模态深度思考模型（混合思考），面向短视频分析与成本敏感场景。

**来源**
- 模型详情（百炼控制台）：<https://bailian.console.aliyun.com/cn-beijing?tab=model#/model-market/detail/qwen3-omni-flash?serviceSite=asia-pacific-china>
- 接入指南：<https://help.aliyun.com/zh/model-studio/qwen-omni>
- 模型价格总表：<https://help.aliyun.com/zh/model-studio/model-pricing>
- 模型选型：<https://help.aliyun.com/zh/model-studio/models>

控制台是 SPA 页面，部分字段（上下文长度、TPM/RPM、限流配额）以「百炼控制台 → 模型详情 → Qwen3-Omni-Flash」当前公示为准；本文记录从公开 help 文档抓到的权威数据。

---

## 1. 模型定位

- **类别**：全模态（Omni）模型，接受文本 + 图片 / 音频 / 视频的多模态输入，输出文本或语音。
- **位置**：Qwen-Omni 三代系列中的 **Flash** 档（成本/吞吐优先）。
  - `Qwen3.5-Omni`：最新一代全模态，长视频/会议纪要，**不支持思考**，**支持联网搜索**。
  - **`Qwen3-Omni-Flash`：混合思考模型，唯一支持思考模式的 Omni 系列，不支持联网搜索。**
  - `Qwen-Omni-Turbo`：已停止更新，建议迁移。
- **目标场景**：短视频分析、低延迟与高并发、对成本敏感的多模态应用。

## 2. 关键限制（Qwen3-Omni-Flash 系列）

| 项 | 值 |
|---|---|
| 输入音视频时长上限 | **≤ 150 秒** |
| 输入模态组合 | **仅支持文本 + 单一其他模态**（图片 **或** 音频 **或** 视频，三选一） |
| 输出模态 | 文本 / 语音 |
| 思考模式 | **支持**（Qwen-Omni 系列中唯一） |
| 联网搜索 | 不支持 |
| 输入音频语种 | 19 种（11 语言 + 8 方言）—— 中、英、德、法、意、泰、韩、日、俄、西、葡 + 川/沪/粤/闽南/陕/宁/津/京 |
| 输出音频语种 | 19 种（同上） |
| 音色数量 | 17–49 种（因快照版本而异） |
| 上下文长度 / TPM / RPM | 控制台公示，本文不固化（百炼控制台 → 模型详情） |

> 与 `Qwen3.5-Omni` 的关键差异：3.5 支持长视频（3 小时音频 / 1 小时视频）和任意多模态组合输入，但**不支持思考**；3-Omni-Flash 反过来——短输入、单一额外模态、**支持思考**。

## 3. 模型快照与价格（来源：模型价格页）

### 3.1 Qwen3-Omni-Flash（在线推理）

**模型 ID**：`qwen3-omni-flash`（指向最新快照 `qwen3-omni-flash-2025-12-01`），其他快照 `qwen3-omni-flash-2025-09-15`。

**计费模式**：非思考与思考模式同价。Token 计价按**输入模态拆分输入单价**，按**输出场景拆分输出单价**。

#### 华北 2（北京）/ 中国内地

| 输入·文本 | 输入·音频 | 输入·图片/视频 | 输出（仅纯文本输入） | 输出（多模态输入） | 输出（文本+音频，仅音频计费） | 免费额度 |
|---|---|---|---|---|---|---|
| **¥1.8 / MTok** | **¥15.8 / MTok** | **¥3.3 / MTok** | **¥6.9 / MTok** | **¥12.7 / MTok** | **¥62.6 / MTok** | 100 万 Token（开通后 90 天内有效）|

#### 新加坡 / 国际

| 输入·文本 | 输入·音频 | 输入·图片/视频 | 输出（仅纯文本输入） | 输出（多模态输入） | 输出（文本+音频，仅音频计费） |
|---|---|---|---|---|---|
| ¥3.156 / MTok | ¥27.962 / MTok | ¥5.725 / MTok | ¥12.183 / MTok | ¥22.458 / MTok | ¥110.896 / MTok |

> **价格映射逻辑**（控制台原表 6 列）：
> - 输入按 token 模态拆 3 档：文本 / 音频 / 图片或视频。
> - 输出按生成场景拆 3 档：
>   1. 「仅纯文本输入 → 文本输出」最便宜（无多模态上下文成本）。
>   2. 「多模态输入 → 文本输出」中档。
>   3. 「多模态输入 → 文本+音频输出」最高，**只对音频部分计费**（文本输出免）。
> - 思考与非思考模式同价。

### 3.2 Qwen3-Omni-Flash-Realtime（实时音视频）

**模型 ID**：`qwen3-omni-flash-realtime` → `qwen3-omni-flash-realtime-2025-12-01`，快照 `…-2025-12-01` / `…-2025-09-15`。

#### 华北 2（北京）/ 中国内地

| 输入·文本 | 输入·音频 | 输入·图片 | 输出（仅纯文本输入→文本） | 输出（多模态输入→文本） | 输出（文本+音频，仅音频计费） | 免费额度 |
|---|---|---|---|---|---|---|
| ¥2.2 / MTok | ¥18.9 / MTok | ¥3.9 / MTok | ¥8.3 / MTok | ¥15.2 / MTok | ¥75.1 / MTok | 100 万 Token |

#### 新加坡 / 国际

| 输入·文本 | 输入·音频 | 输入·图片 | 输出（仅纯文本输入→文本） | 输出（多模态输入→文本） | 输出（文本+音频，仅音频计费） |
|---|---|---|---|---|---|
| ¥3.816 / MTok | ¥33.54 / MTok | ¥6.899 / MTok | ¥14.605 / MTok | ¥26.935 / MTok | ¥133.06 / MTok |

> Realtime 版本输入仅支持文本 / 图片 / 音频（无视频）。

### 3.3 其他同代衍生模型（仅作对照）

| 模型 ID | 用途 | 中国内地·输入·音频 | 中国内地·最高档输出 |
|---|---|---|---|
| `qwen3.5-omni-flash` | 非思考全模态 | ¥18 / MTok | ¥72 / MTok |
| `qwen3.5-omni-flash-realtime` | 非思考实时 | ¥27 / MTok | ¥107 / MTok |
| `qwen3-omni-30b-a3b-captioner` | 音视频描述（开源 captioner） | — | ¥12.7 / MTok（输出） |

> 完整快照价目以「模型价格」页为准。本文聚焦 Qwen3-Omni-Flash 及其 Realtime 变体。

## 4. API 接入

### 4.1 入口

- **协议**：仅支持 OpenAI 兼容方式调用。
- **SDK 版本**：OpenAI Python ≥ 1.52.0，Node.js ≥ 4.68.0。
- **Base URL**：`https://dashscope.aliyuncs.com/compatible-mode/v1`
- **鉴权**：`DASHSCOPE_API_KEY` 环境变量（北京 / 新加坡 Key 不通用，按地域签发）。

### 4.2 必填项

- `stream: true`（**所有 Omni 请求必须流式**）。
- `modalities`：输出模态数组，例如 `["text", "audio"]`。
- 若输出音频：`audio: { voice: "<voice>", format: "wav" }`。
- 若启用思考：通常通过 provider 扩展参数（`enable_thinking` / `extra_body`）开启；具体名称以最新 SDK 文档为准。

### 4.3 Python 最小示例

```python
import os
import base64
import numpy as np
import soundfile as sf
from openai import OpenAI

client = OpenAI(
    api_key=os.getenv("DASHSCOPE_API_KEY"),
    base_url="https://dashscope.aliyuncs.com/compatible-mode/v1",
)

completion = client.chat.completions.create(
    model="qwen3-omni-flash",
    messages=[{"role": "user", "content": "你是谁"}],
    modalities=["text", "audio"],
    audio={"voice": "Tina", "format": "wav"},
    stream=True,
    stream_options={"include_usage": True},
)

audio_b64 = ""
for chunk in completion:
    delta = chunk.choices[0].delta if chunk.choices else None
    if delta and delta.content:
        print(delta.content, end="")
    if delta and getattr(delta, "audio", None):
        audio_b64 += delta.audio.get("data", "")

if audio_b64:
    pcm = np.frombuffer(base64.b64decode(audio_b64), dtype=np.int16)
    sf.write("reply.wav", pcm, samplerate=24000)
```

### 4.4 多模态输入示例（文本 + 音频）

```python
completion = client.chat.completions.create(
    model="qwen3-omni-flash",
    messages=[{
        "role": "user",
        "content": [
            {"type": "input_audio",
             "input_audio": {"data": f"data:audio/wav;base64,{audio_b64}", "format": "wav"}},
            {"type": "text", "text": "把这段音频按时间戳转写为字幕"},
        ],
    }],
    modalities=["text"],
    stream=True,
    stream_options={"include_usage": True},
)
```

> ⚠️ Qwen3-Omni-Flash **不能**同时传图片 + 音频 + 视频；同一请求最多 "文本 + 一种其他模态"。需要任意组合请改用 `qwen3.5-omni-plus` / `qwen3.5-omni-flash`（不支持思考）。

## 5. 使用建议（官方 Prompt 指南）

### 5.1 视频长度档位

| 场景 | 推荐视频长度 | Prompt 建议 | `max_pixels` 推荐 |
|---|---|---|---|
| 快速审核、成本低 | ≤ 60 分钟（分段处理） | < 50 词的简单 Prompt | 230,400 |
| 内容提取（长视频分段） | ≤ 60 分钟 | < 50 词的简单 Prompt | 921,600 – 2,073,600 |
| 标准分析（短视频打标） | ≤ 4 分钟 | 使用结构化 Prompt | 921,600 – 2,073,600 |
| 精细分析（多说话人/复杂场景） | ≤ 2 分钟 | 使用结构化 Prompt | 2,073,600 |

> Qwen3-Omni-Flash 本身硬上限是 **150 秒**；上述长视频档位适用于 `qwen3.5-omni-plus/flash`，本文保留以便选型对照。

### 5.2 音频长度档位

| 场景 | 推荐音频长度 | Prompt 建议 |
|---|---|---|
| 快速审核、低成本 | ≤ 60 分钟（分段） | < 50 词的简单 Prompt |
| 内容提取（长音频分段） | ≤ 60 分钟 | < 50 词的简单 Prompt |
| 标准分析（音频打标） | ≤ 2 分钟 | 结构化 Prompt |
| 精细分析（多说话人/复杂场景） | ≤ 1 分钟 | 结构化 Prompt |

## 6. 选型建议

| 需求 | 推荐模型 |
|---|---|
| 短视频（≤150s）+ 思考 + 多模态理解 | **qwen3-omni-flash** |
| 实时语音/视频低延迟交互 | **qwen3-omni-flash-realtime** |
| 长视频（>150s）/ 多种模态同时输入 / 联网搜索 | `qwen3.5-omni-plus` 或 `qwen3.5-omni-flash` |
| 仅音视频转字幕 / Caption | `qwen3-omni-30b-a3b-captioner` |
| 已在用 `qwen-omni-turbo` | 迁移至 Qwen3-Omni-Flash 或 Qwen3.5-Omni |

## 7. 抓取说明

- 抓取日期：2026-06-25。
- 控制台 SPA 不可被 reader 抓取，所有数值来自 help 文档 (`/zh/model-studio/qwen-omni`、`/zh/model-studio/model-pricing`) 的 Markdown 视图。
- 价格表保留中国内地与新加坡两个地域；其他地域请直接参考「模型价格」页。
- 上下文长度 / TPM / RPM 不在公开 help 文档披露，以控制台模型详情页为准。
