# v0.7.1 Spec：Image AIGC Gateway

## 类型

卫星迭代——与 v0.7 主线并行，独立 crate，不阻塞也不依赖主线进度。

## 背景

Agent 应用场景中图像生成/编辑是高频需求。当前 runtime 没有统一的 AIGC 图像能力，用户只能通过 MCP 或自定义 tool 对接各家 provider，重复处理 API 差异、资产持久化、URL 过期等问题。

## 目标

提供统一的图像生成/编辑 gateway，跨 provider 归一化能力概念（text-to-image、image-to-image、masked edit、region edit 等），provider adapter 翻译为各自协议。

## 范围

新建独立 crate `agent-runtime-aigc-providers`，零 workspace-internal 依赖。

初始 provider：
- Crazyrouter GPT Image API
- Alibaba Cloud DashScope（Qwen-Image / Z-Image / Wan）
- OpenRouter image-output models
- Renderful image generation gateway

核心模块：
- `ImageProvider` trait — adapter 边界
- `ImageGateway` — 编排层（校验、调用 adapter、持久化、事件发射）
- `AssetStore` trait — 资产持久化抽象（OSS / Local / Noop）
- `AssetRegistry` trait — 资产元数据注册与查询
- 归一化请求/响应类型（`ImageGenerationRequest` / `ImageGenerationResponse`）
- `CompatibilityPolicy`（Strict / Coerce）+ `OptionAdjustment` 记录

首个里程碑只覆盖图像。视频、音频、3D 等复用 job / asset / storage / error 基础，不在本迭代范围。

## 设计文档

详细设计见 [Image AIGC Gateway Design](../../superpowers/specs/2026-05-25-image-aigc-gateway-design.md)，覆盖：
- 公共 API 形态（ImageProvider trait、Gateway 编排、请求/响应类型）
- 4 个 provider 的字段映射规则
- 资产持久化（AssetStore + AssetRegistry + 公共输出规则）
- 兼容策略（Strict / Coerce）
- 能力元数据（ImageModelCapabilities）
- 错误处理、可观测性、测试策略

## 与主线的关系

- **不依赖** v0.7 Hook 框架——gateway 是独立 crate
- **不阻塞** v0.7 主线——可完全并行开发
- **后续集成**：v0.7 或之后，core 可将 gateway 包装为内置 tool（`image_generate` / `image_edit`），通过 `AgentConfig` 注册

## 验收标准

- [ ] `agent-runtime-aigc-providers` crate 存在，零 workspace-internal 依赖
- [ ] `ImageProvider` trait 定义完整
- [ ] 4 个 provider adapter 各自通过请求构造 + 响应解析单元测试
- [ ] `AssetStore` 有 `OssAssetStore`、`LocalAssetStore`、`NoopAssetStore` 三种实现
- [ ] `AssetRegistry` 有可用于测试和本地集成的具体实现
- [ ] 公共响应不暴露 provider 原始 URL
- [ ] `GeneratedImage.asset_id` 在 URL 和 Base64 delivery 中都存在
- [ ] URL 输出可直接 fetch，不需要 bucket、object key、endpoint、storage credentials 或签名逻辑
- [ ] `asset_id` 可在正确 scope 下刷新为新的可用 URL，错误 scope 被拒绝
- [ ] `CompatibilityPolicy::Strict` 和 `Coerce` 行为有测试覆盖
- [ ] `cargo test -p agent-runtime-aigc-providers` 全绿
- [ ] `cargo clippy -p agent-runtime-aigc-providers -- -D warnings` 全绿

## Issues 拆解

| Issue | 标题 | 核心交付 |
|-------|------|----------|
| [001](./issues/001-types-and-scaffold/spec.md) | Crate Scaffold and Image Gateway Public Types | 独立 crate、公共类型、provider/gateway 边界、serde 测试 |
| [002](./issues/002-asset-store-and-registry/spec.md) | Asset Store and Registry Foundation | `AssetStore`、`AssetRegistry`、OSS/Local/Noop、`asset_id` 解析 |
| [003](./issues/003-crazyrouter-adapter/spec.md) | Crazyrouter GPT Image Adapter | Crazyrouter 生图/编辑 adapter、multipart、stream partial 解析 |
| [004](./issues/004-aliyun-adapter/spec.md) | Alibaba Cloud DashScope Image Adapter | Qwen/Z-Image/Wan 同步与异步 adapter、URL 过期处理 |
| [005](./issues/005-openrouter-adapter/spec.md) | OpenRouter Image Adapter | Chat Completions 图像输出、`image_config`、stream `delta.images` |
| [006](./issues/006-renderful-adapter/spec.md) | Renderful Image Adapter | Renderful async generation、model metadata、upload/outputs |
| [007](./issues/007-image-gateway-orchestration/spec.md) | Image Gateway Orchestration and Public Output Contract | Gateway 端到端编排、持久化、公共输出 contract |
| [008](./issues/008-factory-telemetry-and-validation/spec.md) | Factory, Telemetry, and End-to-End Validation | Provider factory、共享 HTTP、telemetry、examples、最终验证 |

## 推荐执行顺序

1. **001 先做**：稳定所有后续 issue 依赖的公共类型和 crate 骨架。
2. **002 紧跟**：资产持久化是 gateway 公共输出 contract 的核心，provider/gateway 测试都需要它。
3. **003-006 可并行**：四个 provider adapter 都依赖 001/002，但彼此独立。
4. **007 在 001/002 后即可启动**：可先用 mock provider 做 gateway 竖切；真实 provider 接入随 003-006 合入扩展测试。
5. **008 收尾**：需要所有 provider 和 gateway 可用后统一 factory、telemetry、examples 和全量验证。

依赖图：

```text
001 ──┬──> 002 ──┬──> 003 ─────┐
      │          ├──> 004 ─────┤
      │          ├──> 005 ─────┤
      │          ├──> 006 ─────┤
      │          └──> 007 ─────┤
      └────────────────────────┴──> 008
```

## v0.7.1 权威顺序

1. `docs/iteration/v0_7_1/issues/*/spec.md` 是实施与验收的第一权威。
2. 本文件约束迭代范围、依赖和成功指标。
3. `docs/superpowers/specs/2026-05-25-image-aigc-gateway-design.md` 是设计参考；若与 issue 验收标准冲突，先更新 issue/spec 再实现。
