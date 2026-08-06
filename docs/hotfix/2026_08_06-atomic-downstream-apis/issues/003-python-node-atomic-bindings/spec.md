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

- [x] Python/TS completion 均支持 model、system、user、api key/env/url、json mode、retry 与 request options，
      返回 text/string，不产生 agent runtime events。
- [x] Python/TS one-shot ASR 均接受 bytes/Uint8Array、format、language、provider、api key/env/url、options；
      省略 provider 时两端都精确选择 `aliyun/qwen-audio-3.0-asr-flash`，不依赖 registry provider 默认，
      并返回 transcript string。
- [x] Python/TS realtime ASR 均提供 async start、async send audio、同步幂等 finish、async wait 与 ordered
      synchronous event callback；命名仅按语言惯例变化，状态与错误条件一致。
- [x] 两端显式 API key 优先于 env；指定 env 缺失时报错且不回退 provider 默认 env。
- [x] 空音频、未知 format/provider、非法 context 与 send-after-finish 均明确报错；send audio 在 channel
      满时异步等待容量，Python GIL/Node event loop 不被阻塞，连续拥塞下 chunk 不丢失且顺序不变。
- [x] start 在 Aliyun `task-started` 后才 resolve；在此之前不得接受或发送用户 audio chunk。
- [x] callback 严格串行且只接受同步返回；Python coroutine/JS Promise 返回值明确报本地类型错。
- [x] public wrapper 捕获 callback 首个异常、触发 finish、停止后续 callback，并由 wait 原样重新抛出；
      Node native callback 不得把异常升级为 uncaught/fatal exception。
- [x] Python 调用阻塞 Rust future 时释放 GIL；Node connection/wait 不阻塞 JS event loop。
- [x] `python/orchest/__init__.py`、`.pyi`、`js/index.ts`、`js/index.d.ts`、`js/native.d.ts` 与 native exports
      一致，compile/type smoke test 覆盖三组 API。
- [x] Python guide 记录 uv path/editable 与 CPython 3.12 重建；TS guide 记录 native build + npm pack +
      downstream tarball install。
- [x] Python/Node 新能力放入 focused binding modules，两个现有 `lib.rs` 只承担模块装配与 exports，不继续
      承载 completion/ASR/session 业务代码。
- [x] `maturin build`（或项目允许的等价命令）、Node native build、Python tests 与 Node type/package smoke
      全部通过。

## Notes

- TTS、Gen、Omni Realtime bindings 留待后续 issue；本次不增加空壳 public functions。
- bindings 不直接依赖 `orchest-provider-http` 或 `orchest-provider-stream`，只经 `orchest-provider` wall。
