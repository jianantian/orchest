# Hotfix 2026-10-01: DeepSeek 模型列表更新与 deepseek-flash 图像输入

## Background

DeepSeek 于 2026-09-10 发布 DeepSeek-V4.1-Flash，官方 API 在线模型收敛为两个：
`deepseek-flash` 与 `deepseek-v4-pro`。Orchest 的 DeepSeek catalog、profile 与定价兜底仍以
`deepseek-v4-flash` / `deepseek-v4-pro` 为准，并保留已下线的 `deepseek-chat` /
`deepseek-reasoner` 识别；示例、SDK 指南与绑定测试也仍在使用这些旧名。

同时 `deepseek-flash` 原生支持图像理解（OpenAI 兼容 Chat Completions 的 `image_url` content
part），而共享 Chat 核当前会把所有 `Image` block 丢弃（v0.15 C5 起记 `OptionAdjustment`），
DeepSeek 用户无法通过 Orchest 发送图片。

## 官方事实（2026-10-01 核对）

来源：

- 模型 & 价格：<https://api-docs.deepseek.com/zh-cn/quick_start/pricing>
  （英文版 <https://api-docs.deepseek.com/quick_start/pricing>）
- 更新日志：<https://api-docs.deepseek.com/updates>
- 思考模式：<https://api-docs.deepseek.com/guides/thinking_mode>
- 图像理解：<https://api-docs.deepseek.com/guides/vision>

| 项目 | `deepseek-flash` | `deepseek-v4-pro` |
|------|------------------|-------------------|
| 模型版本 | DeepSeek-V4.1-Flash | DeepSeek-V4-Pro-0813 |
| 思考模式 | 支持非思考与思考（默认） | 同左 |
| 思考强度 | `reasoning_effort`: `low` / `high` / `max` | 同左 |
| 上下文 / 最大输出 | 1M / 384K | 1M / 384K |
| 图像理解 | 支持 | 不支持 |
| 价格（元/百万 tokens，高峰） | 输入 2（缓存命中 0.04）/ 输出 8 | 输入 9（缓存命中 0.30）/ 输出 27 |
| 价格（空闲时段） | 高峰价的一半 | 高峰价的一半 |

旧模型名：

- `deepseek-v4-flash`、`deepseek-v4-flash-vision-exp`：对应模型已下线，但模型名**仍可调用**，
  请求由 DeepSeek-V4.1-Flash 提供服务并按 Flash 价格计费（官方明确的路由别名）。
- `deepseek-chat`、`deepseek-reasoner`：2026-04-24 公告于 2026-07-24 停用，现行价格页已不再列出。

图像输入（`deepseek-flash`）：

- OpenAI 格式：user 消息 `content` 为数组，`{"type":"text","text":...}` +
  `{"type":"image_url","image_url":{"url":"https://...|data:image/...;base64,...","detail"?}}`；
  另支持 Files API `{"type":"file","file_id":...}`。
- `detail`：`low` / `high` / `original` / `auto`。
- 格式 JPEG / PNG / GIF / WebP（按内容检测）；外链 URL ≤ 8192 字符；请求体 ≤ 48 MiB；
  单图 ≤ 32 MiB（base64 / URL）；每请求 ≤ 600 张；单边 ≤ 8192 px（≥15 张时 4096 px）。
- 图像仅允许出现在 `user` 消息中，`system` / `assistant` 中出现返回 400。

## Goals

1. catalog 只列出两个在线模型，能力（thinking、effort、上下文、输出、输入模态、价格）与官方一致。
2. 已下线模型名按官方状态处理：官方声明的路由别名保留 Flash 能力识别但不进 catalog；
   已停用的 `deepseek-chat` / `deepseek-reasoner` 不再被识别为已知模型。
3. 共享 Chat 核获得一个 profile 级多模态 content-part 编码 hook；DeepSeek 用它把 `Image` block
   编码为 `image_url` part；不支持图像的模型在 `Strict` 下请求前报错、`Coerce` 下可见丢弃。
4. 测试、示例、SDK 指南、绑定测试与 CHANGELOG 同步。

## Non-Goals

- 不实现 DeepSeek Files API 上传、Anthropic 兼容端点或 Responses API。
- 不在本地校验图片大小 / 像素 / 数量限制（交由 provider 返回错误）。
- 不改 OpenAI / Volcengine / OpenRouter 的 Chat 图像行为（默认 hook 保持现有丢弃语义）。
- 不建模峰谷分时价格（`ModelPricing` 只支持按输入长度分档）；catalog 取高峰价作保守预算。
- 不调用真实 DeepSeek API；全部用 mock HTTP / 请求体断言验证。

## Issues

| # | Issue | GitHub | 依赖 |
|---|-------|--------|------|
| 001 | [DeepSeek 模型列表更新](issues/001-deepseek-model-list/spec.md) | [#318](https://github.com/jianantian/orchest/issues/318) | — |
| 002 | [deepseek-flash 图像输入](issues/002-deepseek-flash-vision/spec.md) | [#319](https://github.com/jianantian/orchest/issues/319) | 001 |
