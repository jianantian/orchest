# 002 · 实施计划

## 先读

- `crates/orchest-protocol/src/{capability,stream}.rs`、`crates/orchest-provider-core/src/{pricing,catalog}.rs`、语音 adapter 的完成路径。

## 修改范围

- 语音用量协议、时长和字符计价、目录定价接线及计量 fixture。

## 步骤

1. 先固定计量来源、单位、缺项和终止口径，再选择与现有终止事件兼容的传递方式。
2. 复用现有 `Pricing::Duration` 与 `Cost`，加入按字符计价及可追踪的定价来源。
3. 对照已知输入、上游覆盖、缺项、取消、失败和重试用例核对结果。

## 验证依据

- Aliyun ASR 的 `usage.duration` 以秒返回，流式值按任务累计。CosyVoice 的 `usage.characters` 是累计计费字符。北京地区的公开标价用于对应的官方端点和目录模型。[流式 ASR](https://www.alibabacloud.com/help/en/model-studio/qwen-audio-asr-streaming-server-events) · [CosyVoice 事件](https://www.alibabacloud.com/help/en/model-studio/cosyvoice-server-events) · [价目表](https://www.alibabacloud.com/help/en/model-studio/model-pricing)
- Deepgram 的最终 `Metadata.duration` 以秒返回。Nova-3 单语流式估算使用公开按量费率。[CloseStream](https://developers.deepgram.com/docs/close-stream) · [价目表](https://deepgram.com/pricing)
- MiniMax 的 `extra_info` 提供音频毫秒数和计费字符数。Speech 2.8 HD 的公开美元报价只用于官方国际端点。[WebSocket](https://platform.minimax.io/docs/api-reference/speech-t2a-websocket) · [价目表](https://platform.minimax.io/docs/pricing/overview)
- Soniox、AssemblyAI 和 Speechmatics 按文档读取上游处理时长。Volcengine 按仓库内的协议文档读取 `audio_info.duration` 和 `usage.text_words`。[Soniox](https://docs.soniox.com/api-reference/stt/websocket-api) · [AssemblyAI](https://www.assemblyai.com/docs/pre-recorded-audio/api-reference/transcripts/get) · [Speechmatics](https://beta-docs.speechmatics.com/speech-to-text/batch/output)
