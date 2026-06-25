# TODO: Provider 统一路线(omni 驱动)

> 状态: Step 2 已完成,Step 3 待排期 | 记录于 2026-06-22
> 这是 v0.9.10(Minimax 接入)之后的两步方向锚。先堆熵、再降熵 —— 用真实 provider 逼出需求,
> 不投机设计抽象。详见各 step。

## 背景:为什么现在的 crate 分类不是合法架构边界

provider code 原本按 4 个**能力模态** crate 切分,v0.9.11 又新增 experimental realtime crate:

| crate | 模态 |
|---|---|
| `agent-runtime-providers` | LLM(文本对话) |
| `agent-runtime-aigc-providers` | 图片 / 视频生成 |
| `agent-runtime-asr-providers` | 语音转文本 |
| `agent-runtime-tts-providers` | 文本转语音 |
| `agent-runtime-realtime-providers` | experimental realtime / omni evidence |

这套"按模态分 crate"被两个真实模型从**两个相反方向**证伪:

- **omni(端到端语音大模型)** —— 一个 session 里 audio(+text) 进 → 推理+工具 → audio(+text) 出,
  全双工可打断。它**不分解**成 ASR→LLM→TTS pipeline,在任何单模态 crate 里都无家可归。
  → 证伪"按模态切"。
- **Chameleon(对话出图模型)** —— 是个对话 LLM,但在 turn 里**原生吐出 image token**,不走任何
  generation 端点。"产出一张图"可以来自对话 turn,也可以来自异步生成任务。
  → 证伪"image/video 永远归 aigc / 按 model-vs-task 切"。

结论:`asr/tts/llm/aigc` 是 **provider 举例**,不是架构边界。真正正交的是两条轴:

```
WHAT 流动(模态)   = 一套共享 content model:Text/Audio/Image/Video/Thinking/ToolUse
                     所有人共用。一张图就是一张图,不管它从 turn 出还是从 task 出。
HOW 交互(能力原语)= 几个 capability trait,provider 按需 opt-in:
  • turn      messages → content-block 流   (LLM / Chameleon / 4o-img)
  • duplex    realtime 双向                 (omni / 流式 ASR / 流式 TTS)
  • gen-task  submit → poll → fetch asset    (image-gen / video-gen / music-gen / async-TTS)
```

provider = 它支持的能力原语集合。crate 边界**只为依赖重量服务**(别让只要 LLM 的人编
websocket/crypto/oss),不为能力分类服务。

---

## Step 2 — Omni 音频端到端接入(已完成:v0.9.11)

v0.9.11 已接入 **Doubao / Volcengine realtime** 作为第一条 omni/realtime 证据路径,拿到
全双工 session、audio in/out、interleaved transcript/text/audio、barge-in/close/error 语义,
作为 Step 3 重切 crate 的证据。不在这一步设计统一抽象。详见
[`docs/archive/iteration/v0_9_11/evidence.md`](../archive/iteration/v0_9_11/evidence.md)。

provider 选择:

- **豆包音频大模型 / Doubao realtime** —— 已完成,基于已有文档 [`docs/external/volceengine/realtime.md`](../external/volceengine/realtime.md)
- **qwen-omni** —— 延后,待补 `docs/external/` 供应商文档后再作为对比 provider

已观测/记录的形态需求:
- 输入:audio chunk + text + (可能) image,可中途 barge-in
- 输出事件:audio chunk / text delta / 自身语音的 transcript / tool_use / thinking
- 会话:realtime 全双工(类似现有 TTS `start_duplex_stream` + ASR full-duplex,但模态双向解锁)

v0.9.10 已为此铺地基:`agent-runtime-model::ContentBlock` 已加齐 `Image/Video/Audio`(模态完整),
omni 输入侧直接复用。

---

## Step 3 — 合 crate / Provider 统一(重构迭代,待排版本号)

有了 Step 1(Minimax 单厂商横跨 3 个 crate、3 套 `api.minimaxi.com` + Bearer + hex 重复)+
Step 2(omni 形态需求)两份证据后,做架构重构:

1. **先抽 `agent-runtime-provider-core`**(或并入 model):把现在多份重复的基建收进去 ——
   HTTP client builder、SSE/stream helper、retry、telemetry/observability、catalog/registry、
   storage/asset 持久化、`AudioData` + hex 解码 helper。
   - 收益:① 干掉真实重复(多份 `http.rs`、`telemetry`/`observability`);
     ② 让 Minimax / Doubao 这种多模态厂商共享一个 `*Client`;③ 不背巨型 crate。
2. **按"交互原语 × 模态"重切**,而非按 asr/tts/llm/aigc:capability trait + flag,
   模态沉到共享 content model。
3. (可选)再加一个**薄 umbrella crate** re-export,给消费者"一个依赖"的手感。
4. **god-trait 风险缓解**:拆半双工 core trait + 全双工 extension trait + capability flag,
   纯文本 adapter 不假装会音频。
5. **迁移**:asr/tts 已发布(v0.9.1 / v0.9.3,有消费者),收敛是 breaking,需 deprecation 路径。

**ADR(`docs/adr/`,本步开始时先立)的硬验收判据**:切法必须同时容得下
- **omni**(跨模态,duplex 行多格)
- **Chameleon**(跨形态,turn × image)

轴选错(按模态 / 按 model-vs-task)会立刻被这两个反例证伪。

---

## 与 v0.9.10 的关系

| Step | 内容 | 类型 | 给 Step 3 的证据 |
|---|---|---|---|
| 1 | Minimax 接入(本次 = v0.9.10) | 功能 | 单厂商跨 crate 的 http+auth 重复 |
| 2 | Omni 接入(Doubao / Volcengine realtime,v0.9.11) | 功能 | omni 全双工全模态形态 |
| 3 | 合 crate / provider 统一 | 重构 | —(消费上面两份证据) |
