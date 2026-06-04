# v0.9.2 Spec：文档 + 发布准备

## 背景

经过 v0.1–v0.9 的迭代，Orchest runtime 的核心能力已具备：run loop、tool system、MCP、provider 抽象、hook 框架、session 持久化、guardrail、supervised delegation。但缺少面向外部用户的文档和正式发布流程。

v0.9.2 作为卫星迭代，补齐文档和发布基础设施，使 Orchest 可以对外发布。

## 目标

1. API 文档可用（rustdoc 生成、关键模块有 module-level 文档）
2. 入门教程和进阶示例覆盖核心功能
3. crates.io 发布流程可用且有 CI 支撑

## 范围

### API 文档

- 关键 public module 补 `//!` module-level 文档（不强求每个函数都有 doc comment，但 pub trait / pub struct / pub enum 需要）
- `cargo doc --workspace --no-deps` 生成无 warning
- README.md 更新：功能概览、快速开始、架构图

### 入门教程

`docs/guide/quickstart.md`：从零到一个可运行的 agent，覆盖：
1. 添加依赖
2. 配置 provider（Anthropic 为默认示例）
3. 注册 tool
4. 启动 run + 监听事件
5. 处理 approval

### 进阶示例

确保 `examples/rust/` 覆盖以下场景（部分已存在，补缺的）：

| 场景 | 状态 |
|------|------|
| 基础 agent run | 需补 |
| 自定义 Hook | ✅ 已有（hook_logging, hook_abort, hook_modifier） |
| Guardrail | ✅ 已有（guardrail_keyword_filter, guardrail_output_sanitize） |
| Session persist + resume | ✅ 已有（session_persist_resume） |
| Handoff | ✅ 已有（handoff_routing, handoff_input_filter） |
| Steering（inject + steer） | v0.9 新增 |
| Supervised Delegation | v0.9 新增 |
| 多 Provider | ✅ 已有（provider_runtime_deepseek, provider_runtime_openrouter） |
| Agent-as-Tool | ✅ 已有（agent_as_tool） |

### SDK 文档

- Python SDK：`docs/guide/sdk-python.md`（安装、基础用法、event 监听）
- TypeScript SDK：`docs/guide/sdk-typescript.md`（同上）

### 发布流程

- `cargo publish --dry-run` 通过（所有 crate）
- 版本号策略文档（semver，何时 bump minor/patch）
- CHANGELOG.md（从 git history 生成初版，后续手动维护）
- CI 新增 release workflow：tag push → cargo publish → GitHub Release

## 不在范围内

- 完整的 book-style 文档站（mdBook / Docusaurus）——初版用 rustdoc + markdown guide
- 视频教程
- 多语言翻译

## 依赖

- v0.9 Supervised Delegation（steering / SD 示例需要 v0.9 API）

## 验收标准

- [ ] `cargo doc --workspace --no-deps` 无 warning
- [ ] `docs/guide/quickstart.md` 存在且代码示例可编译
- [ ] `docs/guide/sdk-python.md` 和 `docs/guide/sdk-typescript.md` 存在
- [ ] `examples/rust/` 覆盖上表所有场景
- [ ] `cargo publish --dry-run` 对所有 crate 通过
- [ ] CHANGELOG.md 存在
- [ ] CI release workflow 配置完成
- [ ] README.md 更新
