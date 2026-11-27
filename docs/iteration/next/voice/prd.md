# 语音能力目录、计量与 TTS 双工

## 背景与目标

v1.2 已补齐现有流式 ASR 的事件与 Finalize 契约，并随 SDK 0.2 交付新接口。当前 ASR/TTS 目录扩展为空，语音流没有统一的音频时长或字符计量。Aliyun、Volcengine、MiniMax 的 TTS 支持一次性和整段文本的流式合成，三个双工入口均返回 `UnsupportedOperation`。[语音迭代计划](../../voice/plan.md) · [ADR-0006](../../../adr/0006-voice-session-model.md)

交付语音线 V2 与 V4。调用方可在连接前查询能力、校验选项、获得可解释的用量，并能在同一 TTS 会话中追加文本、读取音频、Flush、取消上下文和关闭。

## 范围与约定

- 完善六个现有 ASR 目录项所覆盖的批量或流式能力，并挂接三个 TTS 方言的模型目录。目录标注可验证的能力、来源与方言。可移植选项经 registry 校验。发现结果不需要凭证。
- 语音用量携带实际服务的 provider / model、音频时长或字符数及来源。已有可靠上游用量时透传，已知输入可给出有来源的估算。缺失的时长、公开单价或结算数据保留未知状态。估算金额随币种和计价单位返回。
- 对齐 TTS 的 Session、Stable、Finalize 与 Turn。音频分片标识所属上下文及可确认的 Flush 边界，存在词级对齐时放进 Extras。厂商的协议差异留在适配器。
- 用真实协议资料或可核对的实测明确各方言的 Flush 边界与取消行为。对于只能结束当前上游 task 的方言，先验证能否透明续接。超出可验证能力的请求返回明确错误，并同步下调目录声明。
- 提供 Python 和 Node 的一次性、整段流式与双工合成。保留已有 ASR API 和 LLM token 计量。聚合网关留给语音线 V3，全双工语音对话留给语音线 V5。

## 发布验收

- [ ] registry 能按所需能力返回相应的 ASR/TTS 模型。测试证明选项在建连前校验，能力目录和对应 adapter 的实测行为一致。
- [ ] 已知长度的音频与多段 Unicode 文本能在流结束时核对用量、实际 provider / model 和估算依据。缺失数据、取消和失败具有可区分的输出。
- [ ] Aliyun、Volcengine、MiniMax 分别通过双工契约测试。调用方在输入仍开放时能收到音频，Flush 的承诺边界可验证，取消确认后无旧上下文音频，关闭释放资源。
- [ ] 两端 SDK 用相同 fixture 验证三种合成方式。三平台构建和安装测试通过。记录三个方言的真实服务验收情况及未覆盖项。

## 任务顺序

| Issue | 交付 | 依赖 |
| --- | --- | --- |
| [001](issues/001-voice-catalog/spec.md) | 能力目录、发现与便携选项校验 | v1.2 |
| [002](issues/002-voice-metering/spec.md) | 语音用量与成本估算 | 001 的模型身份与定价字段 |
| [003](issues/003-tts-duplex/spec.md) | TTS 双工契约与三个方言 | v1.2，目录与计量接线依赖 001、002 |
| [004](issues/004-tts-bindings/spec.md) | Python / Node 合成 API 与 SDK 验收 | 001、002、003 |

003 的协议验证可与 001、002 并行。Rust 提供原生语音能力，Python / Node 的公开包在通过 004 验收后交付 TTS 接口。[下一版发布计划](../release-plan.md)
