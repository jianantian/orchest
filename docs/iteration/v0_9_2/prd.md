# v0.9.2 Spec：文档

## 背景

经过 v0.1–v0.9 的迭代，Orchest runtime 的核心能力已具备：run loop、tool system、MCP、provider 抽象、hook 框架、session 持久化、guardrail、supervised delegation。但缺少面向外部用户的学习文档。

v0.9.2 作为卫星迭代，补齐 API 文档、入门教程和 SDK 指南，让新用户能快速理解和上手 Orchest。

### 发布与验证节奏（2026-06-06 调整）

- **crates.io 公开发布推迟到 v1.0**——v1.0 是第一个公开发布版本。所有发布准备（Cargo publish 元数据、license 定稿、release workflow、CHANGELOG）归入 v1.0，不在 v0.9.2 范围。
- **v1.0 之前先做一个 SDK 验证产品**（独立迭代，dogfooding）：用一个真实的简单产品验证 SDK 是否完备、易用。验证结论可能反过来影响 API 形状，因此发布必须排在验证之后。该验证迭代的 SDK 语言、产品形态待单独规划。
- 因此 **v0.9.2 收缩为纯文档迭代**，不含任何发布准备。

### 当前文档状态审计（2026-06-06）

| 项目 | 状态 |
|------|------|
| README.md | ✅ 已更新（含架构、快速开始、项目结构） |
| examples/rust/ | ⚠️ 8/9 场景，缺"基础 agent run" |
| docs/guide/ | ❌ 目录不存在 |
| cargo doc 无 warning | ❌ 1 处 warning + 4 个 crate 缺 module-level doc |

## 目标

1. API 文档可用（rustdoc 生成无 warning、关键模块有 module-level 文档）
2. 入门教程和进阶示例覆盖核心功能
3. SDK 文档（Python / TypeScript）可用

## 范围

### API 文档

**目标状态**：`cargo doc --workspace --no-deps` 无任何 warning，关键公共类型有 doc comment。

**当前已知问题：**

| 位置 | 问题 |
|------|------|
| `crates/agent-runtime-core/src/run/agent_ref.rs:1` | rustdoc unclosed HTML tag：`AgentMsg` 应用反引号而非裸 HTML |
| `crates/agent-runtime-aigc-providers/src/lib.rs` | 缺 `//!` module-level doc |
| `crates/agent-runtime-asr-providers/src/lib.rs` | 缺 `//!` module-level doc |
| `crates/agent-runtime-node/src/lib.rs` | 缺 `//!` module-level doc |
| `crates/agent-runtime-py/src/lib.rs` | 缺 `//!` module-level doc |

**覆盖要求**：pub trait / pub struct / pub enum 须有 `///` doc comment，解释类型的职责和使用场景。pub fn 不强制，但核心公共 API（如 `AgentRun::start`、`RunHandle::subscribe_events`）须有。

### 入门教程

`docs/guide/quickstart.md`：从零到一个可运行的 agent，覆盖：

1. 添加 Cargo 依赖
2. 配置 provider（Anthropic 为默认示例，含 env var 用法）
3. 注册 tool（用最简形式展示 tool call 流程）
4. 启动 run + 监听事件（`RunHandle::subscribe_events`）
5. 等待 run 完成，处理结果

**可编译验证**：quickstart 的代码示例须与 `examples/rust/basic_agent_run.rs` 对应（可直接引用或保持一致），CI 通过 `cargo build --example basic_agent_run` 验证。

依赖版本号：v0.9.2 未发布到 crates.io，quickstart 暂用 git / path 依赖示例，并注明"crates.io 版本待 v1.0 发布后更新"。

### 进阶示例

确保 `examples/rust/` 覆盖以下场景（部分已存在，补缺）：

| 场景 | 状态 |
|------|------|
| 基础 agent run | **需新增** `basic_agent_run.rs` |
| 自定义 Hook | ✅ 已有（hook_logging, hook_abort, hook_modifier） |
| Guardrail | ✅ 已有（guardrail_keyword_filter, guardrail_output_sanitize） |
| Session persist + resume | ✅ 已有（session_persist_resume） |
| Handoff | ✅ 已有（handoff_routing, handoff_input_filter） |
| Steering（inject + steer） | ✅ 已有（watcher_inject_message, watcher_abort_on_pattern） |
| Supervised Delegation | ✅ 已有（supervised_delegation） |
| 多 Provider | ✅ 已有（provider_runtime_deepseek, provider_runtime_openrouter） |
| Agent-as-Tool | ✅ 已有（agent_as_tool） |

**`basic_agent_run.rs` 规格：**

- 不使用 Hook / Guardrail / Session / Handoff 等进阶功能
- 展示最小可用配置：provider 构造 → ToolRegistry → AgentConfig → AgentRun::start() → 事件订阅 → 等待完成
- 包含一个最简 tool（如 `get_current_time`）以展示完整的 tool call 流程
- 注释说明每一步作用（example 面向初学者，例外于"不写注释"原则）
- 须在 `crates/agent-runtime-core/Cargo.toml` 的 `[[example]]` 中注册

### SDK 文档

`docs/guide/sdk-python.md`：
- 安装（`uvx maturin develop` 开发模式）
- 基础用法（`Agent` 构造、`run()` 调用、event loop 迭代）
- Tool 注册（Python 函数转 tool 的方式）
- 常用 event 类型速查表

`docs/guide/sdk-typescript.md`：
- 安装（`npm install && npm run build:native`）
- 基础用法（`Agent` 构造、`runSync()` / `run()` 调用）
- Tool 注册
- 类型声明说明

SDK 文档的代码示例与 README.md Quick start 保持一致，README 提供最短片段，sdk-*.md 提供完整可运行版本。

## 不在范围内

- 完整的 book-style 文档站（mdBook / Docusaurus）——初版用 rustdoc + markdown guide
- 视频教程
- 多语言翻译
- **crates.io 发布、release workflow、Cargo publish 元数据、license 定稿、CHANGELOG** —— 全部归入 **v1.0**（见下）
- **SDK 验证产品** —— 独立验证迭代，SDK 语言与产品形态待单独规划

### 推迟到 v1.0 的发布准备

以下原属"发布准备"的工作整体移到 v1.0（第一个公开发布版本），且须排在 SDK 验证产品迭代之后：

- 各发布 crate 的 Cargo 元数据（description / license-file / repository / readme / categories / keywords）
- license 最终确定（README 当前为 "UNLICENSED — internal development"；正式 license 在 v1.0 定）
- binding crate（py / node）的 `publish = false` 标记
- crate 发布顺序（model / asr → providers / aigc → core）
- `.github/workflows/release.yml`（tag push → cargo publish → GitHub Release）
- `CHANGELOG.md`
- `docs/guide/versioning.md`（semver 策略、版本号与迭代编号对应关系）

## 依赖

- v0.9 Supervised Delegation 已完成（steering / SD 示例需要 v0.9 API）

## 测试策略

| 验证项 | 方法 |
|--------|------|
| cargo doc 无 warning | CI 中 `cargo doc --workspace --no-deps 2>&1 \| grep warning` 返回空 |
| quickstart 可编译 | CI 中 `cargo build --example basic_agent_run` 通过 |
| examples 全部可编译 | CI 中 `cargo build --examples` 通过 |

## Issue Breakdown

| Issue | 标题 | 依赖 | 范围 |
|-------|------|------|------|
| 001 | cargo-doc 清理 | — | 修复 agent_ref.rs rustdoc warning；补 aigc-providers / asr-providers / node / py 四个 crate 的 `//!` module-level doc；pub 类型 doc comment 补齐 |
| 002 | basic_agent_run 示例 | — | 新增 `examples/rust/basic_agent_run.rs`；注册到 `agent-runtime-core/Cargo.toml [[example]]` |
| 003 | 入门教程 | 002 | `docs/guide/quickstart.md`，代码示例与 basic_agent_run 对应 |
| 004 | SDK 文档 | — | `docs/guide/sdk-python.md`、`docs/guide/sdk-typescript.md` |

**依赖图：**

```
001 (cargo-doc)
002 (basic example) ──► 003 (quickstart)
004 (sdk-docs)
```

001 / 002 / 004 相互独立，可并行开发。003 依赖 002。

## 验收标准

### API 文档
- [ ] `cargo doc --workspace --no-deps` 无 warning
- [ ] `agent-runtime-aigc-providers` / `agent-runtime-asr-providers` / `agent-runtime-node` / `agent-runtime-py` 四个 crate 的 `lib.rs` 有 `//!` module-level doc

### 示例 + 文档
- [ ] `examples/rust/basic_agent_run.rs` 存在，`cargo build --example basic_agent_run` 通过
- [ ] `docs/guide/quickstart.md` 存在，代码示例与 basic_agent_run 对应
- [ ] `docs/guide/sdk-python.md` 存在
- [ ] `docs/guide/sdk-typescript.md` 存在
- [ ] `examples/rust/` 覆盖上表全部 9 个场景

### CI 基线
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `cargo build --examples` 全部通过
