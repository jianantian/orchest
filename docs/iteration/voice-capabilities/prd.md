# 语音能力目录、计量与 TTS 双工

## 背景与目标

v1.2 已补齐现有流式 ASR 的事件与 Finalize 契约，并随 SDK 0.2 交付新接口。当前 ASR/TTS 目录扩展为空，语音流没有统一的音频时长或字符计量。Aliyun、Volcengine、MiniMax 的 TTS 支持一次性和整段文本的流式合成，三个双工入口均返回 `UnsupportedOperation`。[语音迭代计划](../voice/plan.md) · [ADR-0006](../../adr/0006-voice-session-model.md)

交付语音线 V2，并按方言验证 V4。调用方用同一套 SDK 入口先查询能力和校验选项，再发起一次性、整段流式或双工合成。已声明的双工操作可在会话内使用，用量和来源可直接读取。

## 范围与约定

- 完善六个现有 ASR 目录项所覆盖的批量或流式能力，并挂接三个 TTS 方言的模型目录。目录标注可验证的能力、来源与方言。可移植选项经 registry 校验。发现结果不需要凭证。
- 语音用量携带实际服务的 provider / model、音频时长或字符数及来源。已有可靠上游用量时透传，已知输入可给出有来源的估算。缺失的时长、公开单价或结算数据保留未知状态。估算金额随币种和计价单位返回。
- 对齐 TTS 的 Session、Stable、Finalize 与 Turn。音频分片标识所属上下文及可确认的 Flush 边界，存在词级对齐时放进 Extras。厂商的协议差异留在适配器。
- 用真实协议资料或可核对的实测明确各方言的 Flush 边界与取消行为。对于只能结束当前上游 task 的方言，先验证能否透明续接。超出可验证能力的请求返回明确错误，并同步下调目录声明。
- 提供 Python 和 Node 的一次性、整段流式与双工合成。保留已有 ASR API 和 LLM token 计量。聚合网关留给语音线 V3，全双工语音对话留给语音线 V5。

## 架构与性能

能力发现、便携选项校验和用量语义由 Rust provider wall 与共享契约统一提供。方言差异由 adapter 处理，绑定以原生字节类型交付音频。流式路径使用有界缓冲，避免整段聚合和重复复制。使用可控 transport 单独度量 SDK 和 adapter 开销，真实服务结果单独记录。

## 迭代验收

- [ ] registry 能按所需能力返回相应的 ASR/TTS 模型。测试证明选项在建连前校验，能力目录和对应 adapter 的实测行为一致。
- [ ] 已知长度的音频与多段 Unicode 文本能在流结束时核对用量、实际 provider / model 和估算依据。缺失数据、取消和失败具有可区分的输出。
- [ ] Aliyun、Volcengine、MiniMax 分别列出双工、Flush、取消及关闭的验证状态。宣称支持双工的方言须在输入仍开放时输出音频、可继续追加并能关闭释放资源。Flush 和取消分别按已验证的承诺验收，超出声明的操作返回明确错误。
- [ ] 两端 SDK 用相同 fixture 验证一次性、整段流式和已声明的双工操作，也覆盖不支持能力时的调用体验。从三个目标平台的本地 SDK 构建产物验证公开入口。
- [ ] 对同一音频负载记录首块延迟、吞吐及慢消费者下的队列峰值，对比 Rust 与两端 SDK。关闭及取消路径记录耗时和资源回收，优化测得的主要复制、排队和调度开销。

## 任务顺序

| Issue | 交付 | 依赖 |
| --- | --- | --- |
| [001](issues/001-voice-catalog/spec.md) · [#371](https://github.com/jianantian/orchest/issues/371) | 能力目录、发现与便携选项校验 | v1.2 |
| [002](issues/002-voice-metering/spec.md) · [#372](https://github.com/jianantian/orchest/issues/372) | 语音用量与成本估算 | 001 的模型身份与定价字段 |
| [003](issues/003-tts-duplex/spec.md) · [#373](https://github.com/jianantian/orchest/issues/373) | TTS 双工契约与三个方言 | v1.2，目录与计量接线依赖 001、002 |
| [004](issues/004-tts-bindings/spec.md) · [#374](https://github.com/jianantian/orchest/issues/374) | Python / Node 合成 API 与 SDK 验收 | 001、002、003 |

003 的协议验证可与 001、002 并行。004 验收 Rust 原生接口在 Python / Node 两端的公开绑定。
