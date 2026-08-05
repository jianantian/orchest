# Aliyun HTTP ASR and Realtime Session Repair

## Background

`orchest-provider-stream` 已有阿里云 inference WebSocket 方言，但 `transcribe()` 固定返回
UnsupportedOperation。最新 DashScope 文档确认 `qwen-audio-3.0-asr-flash` 可通过同步
multimodal-generation HTTP 接收 URL/Base64，并支持 m4a、language hints、热词与上下文。

现有 realtime `build_run_task()` 将所有 options 平铺到 `parameters` 且固定 `input:{}`，因此
`context` 无法按官方 wire shape 生效。

## Goal / Scope

- 在 `orchest-provider-http` 新增 Aliyun synchronous ASR adapter 与 catalog rows。
- 协议 `AudioFormat` 增加 `M4a`、`Aac`。
- 修复 realtime run-task 的 context/parameter 分流，并提供绑定可安全持有的 session 状态语义。
- 保持 provider implementation 只依赖 `orchest-protocol` + `orchest-provider-core`，经 wall 注册。

## Acceptance Criteria

- [ ] HTTP catalog 公开 `aliyun/qwen-audio-3.0-asr-flash` 为 provider 默认，并公开非默认
      `aliyun/fun-asr-flash-2026-06-15`。
- [ ] registry `.asr().id("aliyun/qwen-audio-3.0-asr-flash")` 构造 HTTP adapter，streaming model
      仍构造 WS adapter，调用方无需命名 impl crate。
- [ ] m4a bytes 生成 `data:audio/mp4;base64,...`，aac 生成 `data:audio/aac;base64,...`。
- [ ] HTTP request method、endpoint、headers、model、input.messages 与 parameters 逐字段 fixture 固定。
- [ ] `language`、sample rate、vocabulary id、inline vocabulary、简化 context message 映射到设计规定位置；
      role/count/order/400 字符约束在发送请求或连接前验证。
- [ ] 公共 endpoint 是默认值；完整 workspace endpoint 通过 `api_url` 原样使用，不重复追加 path。
- [ ] response fixture 的 `output.text`、request id、usage 与 sentence detail 正确映射；缺失 text 报协议错。
- [ ] HTTP ASR `start_stream()` 明确 unsupported；WS ASR `transcribe()` 仍明确 unsupported。
- [ ] realtime run-task 把 context 放入 `payload.input.context`，不把 context 混入 parameters。
- [ ] realtime format/sample rate/language hints/vocabulary 保持在 parameters；context 约束失败在连接前报错。
- [ ] session Open/Finishing/Closed 状态遵循 PRD：finish 幂等、send-after-finish 失败、背压不丢 chunk、
      fatal error 令 wait 失败、drop best-effort finish。
- [ ] request/error diagnostics 不含 API key 或 Base64 音频正文。
- [ ] HTTP 与 WS provider tests、catalog/registry tests 全部通过。

## Notes

- 不实现 filetrans/OSS/轮询、转码、采样探测或动态 continue-task。
- 阿里云文档对同步 Base64 大小表述存在 10MB/2GB 不一致；SDK 不做本地大小上限裁决，交由 provider。
