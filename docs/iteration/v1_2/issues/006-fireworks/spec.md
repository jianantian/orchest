# 006 · Fireworks：chat

GitHub issue: #347

## Background

owner 要求接入 [Fireworks](https://app.fireworks.ai/models)。它提供 OpenAI 兼容的 chat 接口，模型 id 形如 `accounts/fireworks/models/<name>`，列表经常变化。

## Goal

`fireworks/<model>` 可用于 chat，接法与 OpenRouter 一致。

## Acceptance Criteria

- [ ] `defaults::fireworks`：默认 API URL 与 `FIREWORKS_API_KEY` 环境变量名，以官方文档为准并在注释里给出链接。
- [ ] `providers/fireworks/`：profile + `build_chat_adapter`，复用共享 `ChatAdapter`。
- [ ] id 规范化：`fireworks/<name>` 映射为上游的 `accounts/fireworks/models/<name>`；已写全路径的 id 原样透传。
- [ ] 不写静态模型目录行；registry 的 provider 列表包含 `fireworks`。
- [ ] 单元测试：key 解析与缺 key 报错、URL 规范化、id 映射、请求体形状（不依赖网络）。
- [ ] 一条带 key 才运行的冒烟测试。
- [ ] `CONVENTIONS.md` 的 provider 列表与 README 的 provider 表更新。

## Blocked by

无。

## Notes

- Fireworks 的流式 ASR、图像生成、embedding 不在本 issue。
- Fireworks 的 decision 延后（owner 2026-10-27）：Fireworks 没有 Decisions API，要做只能在 chat 上自建，留到后续迭代。
