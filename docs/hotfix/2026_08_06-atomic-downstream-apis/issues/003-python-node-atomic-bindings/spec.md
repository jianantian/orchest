# Python and TypeScript Atomic Capability Bindings

## Background

Rust traits/registry 即使具备原子能力，Python/TypeScript 下游仍无法直接使用。两种绑定必须同批
交付，避免一个 SDK 继续只能构造 Agent，或两端出现不兼容的 session/error 语义。

## Goal / Scope

- Python 公开 `complete`、`transcribe`、`start_asr_stream`、`AsrStream`。
- Node addon/TypeScript 公开 `complete`、`transcribe`、`startAsrStream`、`AsrStream`。
- 两端共享 core/provider 逻辑，只做命名、类型、runtime/callback 与异常转换。
- 更新 Python/TS 类型声明、包导出、SDK guide 与下游本地安装说明。

## Acceptance Criteria

- [ ] Python/TS completion 均支持 model、system、user、api key/env/url、json mode、retry 与 request options，
      返回 text/string，不产生 agent runtime events。
- [ ] Python/TS one-shot ASR 均接受 bytes/Uint8Array、format、language、provider、api key/env/url、options，
      默认模型相同并返回 transcript string。
- [ ] Python/TS realtime ASR 均提供 start、send audio、finish、wait 与 ordered event callback；命名仅按语言
      惯例变化，状态与错误条件一致。
- [ ] 两端显式 API key 优先于 env；指定 env 缺失时报错且不回退 provider 默认 env。
- [ ] 空音频、未知 format/provider、非法 context、send-after-finish 与 channel backpressure 均明确报错。
- [ ] callback 抛错终止 pump 并由 wait 观察，不继续静默消费事件。
- [ ] Python 调用阻塞 Rust future 时释放 GIL；Node connection/wait 不阻塞 JS event loop。
- [ ] `python/orchest/__init__.py`、`.pyi`、`js/index.ts`、`js/index.d.ts`、`js/native.d.ts` 与 native exports
      一致，compile/type smoke test 覆盖三组 API。
- [ ] Python guide 记录 uv path/editable 与 CPython 3.12 重建；TS guide 记录 native build + npm pack +
      downstream tarball install。
- [ ] `maturin build`（或项目允许的等价命令）、Node native build、Python tests 与 Node type/package smoke
      全部通过。

## Notes

- TTS、Gen、Omni Realtime bindings 留待后续 issue；本次不增加空壳 public functions。
- bindings 不直接依赖 `orchest-provider-http` 或 `orchest-provider-stream`，只经 `orchest-provider` wall。

