# v0.9.1 Spec：Provider 扩展

## 背景

当前 `agent-runtime-providers` crate 支持 4 个 provider：Anthropic、OpenAI、DeepSeek、OpenRouter。hotfix-0526 落地的 `ProviderFactory` trait 使新增 provider 只需实现 trait，不改 core。

v0.9.1 作为卫星迭代，扩展 provider 覆盖到主流模型，与 v0.9 主线（Supervised Delegation）并行推进。

## 目标

新增 3 个 provider adapter，覆盖主流 LLM API。

## 范围

| Provider | 优先级 | 说明 | 实现策略 |
|----------|--------|------|---------|
| Google Gemini | P0 | 主流，API 差异较大（`generateContent` 端点、`Part` 结构） | 独立 adapter |
| Ollama / 本地模型 | P1 | 本地部署场景 | 复用 `openai_compat`（Ollama 兼容 OpenAI API） |
| Mistral | P2 | 云端 + 本地 | 复用 `openai_compat` |

### Google Gemini Adapter

- 实现 `ProviderFactory` + `ModelAdapter` for Gemini
- 处理 API 差异：`Part` 结构 ↔ `ContentBlock` 转换、tool calling 格式、safety settings
- 支持 `gemini-2.5-pro`、`gemini-2.5-flash` 等主流模型
- feature flag: `gemini`

### Ollama Adapter

- 复用 `openai_compat` 模块，配置 base URL 为本地 Ollama 端点
- 处理 Ollama 特殊行为（无 usage 统计、streaming 差异）
- 支持 `llama3`、`codellama`、`mistral` 等本地模型
- feature flag: `ollama`

### Mistral Adapter

- 复用 `openai_compat` 模块
- Mistral API 基本兼容 OpenAI，主要差异在 tool_choice 枚举和 usage 字段
- 支持 `mistral-large`、`codestral` 等模型
- feature flag: `mistral`

## 不在范围内

- Core 修改（所有 adapter 都通过 `ProviderFactory` 接入）
- Provider 选择策略 / 路由（multi-provider routing 是独立课题）
- 定价数据（adapter 不硬编码定价，由调用方通过 `BudgetGuard` 配置）

## 依赖

- `ProviderFactory` trait（hotfix-0526 已落地）
- `openai_compat` 模块（v0.5 已落地）

## 验收标准

- [ ] Google Gemini adapter 通过集成测试（需 API key）
- [ ] Ollama adapter 通过本地模型集成测试
- [ ] Mistral adapter 通过集成测试（需 API key）
- [ ] 每个 adapter 注册到 `ProviderRegistry`
- [ ] `examples/rust/` 新增对应 provider 示例
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
