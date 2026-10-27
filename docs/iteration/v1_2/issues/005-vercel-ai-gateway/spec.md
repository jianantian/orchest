# 005 · Vercel AI Gateway：chat 与 decision

GitHub issue: #346

## Background

owner 要求接入 [Vercel AI Gateway](https://vercel.com/docs/ai-gateway)。它提供 OpenAI 兼容的 chat 接口，模型 id 形如 `<vendor>/<model>`，列表随上游变化，和已接入的 OpenRouter 同属动态网关（`crates/orchest-provider-http/src/providers/openrouter/`）。

## Goal

`vercel/<vendor>/<model>` 可用于 chat 与 decision，接法与 OpenRouter 一致。

## Acceptance Criteria

- [ ] `defaults::vercel`：默认 API URL 与 `AI_GATEWAY_API_KEY` 环境变量名，以官方文档为准并在注释里给出链接。
- [ ] `providers/vercel/`：profile + `build_chat_adapter`，复用共享 `ChatAdapter`；与 OpenAI 协议的偏差写在 profile 里。
- [ ] `normalize_provider_model` 保留 `vercel/<vendor>/<model>` 的完整 id。
- [ ] decision 能力桶可选 Vercel。
- [ ] 不写静态模型目录行；registry 的 provider 列表包含 `vercel`。
- [ ] 单元测试：key 解析与缺 key 报错、URL 规范化、请求体形状、id 规范化（不依赖网络）。
- [ ] 一条带 key 才运行的冒烟测试（`#[ignore]` 或环境变量开关）。
- [ ] `CONVENTIONS.md` 的 provider 列表与 README 的 provider 表更新。

## Blocked by

无。

## Notes

- Vercel 的 transcribe / speech / realtime 属语音 V3，不在本 issue。
