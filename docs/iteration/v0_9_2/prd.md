# v0.9.2 Spec：文档 + 发布准备

## 背景

经过 v0.1–v0.9 的迭代，Orchest runtime 的核心能力已具备：run loop、tool system、MCP、provider 抽象、hook 框架、session 持久化、guardrail、supervised delegation。但缺少面向外部用户的文档和正式发布流程。

v0.9.2 作为卫星迭代，补齐文档和发布基础设施，使 Orchest 可以对外发布。

**当前状态（代码库审计，2026-06-06）：**

| 项目 | 状态 |
|------|------|
| README.md | ✅ 已更新（含架构、快速开始、项目结构） |
| examples/rust/ | ⚠️ 8/9 场景，缺"基础 agent run" |
| docs/guide/ | ❌ 目录不存在 |
| cargo doc 无 warning | ❌ 1 处 warning + 4 个 crate 缺 module-level doc |
| Cargo.toml 元数据 | ❌ 7 个 crate 均缺 description / license / repository / readme |
| CHANGELOG.md | ❌ 不存在 |
| Release workflow | ❌ .github/workflows/ 只有 ci.yml |

## 目标

1. API 文档可用（rustdoc 生成无 warning、关键模块有 module-level 文档）
2. 入门教程和进阶示例覆盖核心功能
3. crates.io 发布流程可用且有 CI 支撑

## 发布约束

### License 决策

当前 README 标注"UNLICENSED — internal development"。crates.io 发布要求 `license` 字段非空。本迭代须选定 license 并落地：

**推荐**：`MIT OR Apache-2.0`（Rust 生态标准双许可；Apache-2.0 含专利保护条款，MIT 最宽松，双许可覆盖两类用户偏好）。

落地清单：
- 所有发布 crate 的 Cargo.toml 加 `license = "MIT OR Apache-2.0"`
- 根目录新增 `LICENSE-MIT` 和 `LICENSE-APACHE`
- README.md 的 License section 由"UNLICENSED"更新为正式许可说明

### 版本号决策

当前所有 crate 版本为 `0.1.0`，迭代编号为 v0.9.2。首次公开发布的版本号须在本迭代确定：

**选项 A（推荐）**：以 `0.9.2` 作为首次发布版本，直接对齐迭代编号。向外部用户传达"这是接近稳定的版本"，避免 0.1.0 暗示的"极早期"印象。

**选项 B**：保持 `0.1.0`，理由是"这是首次公开发布"。如选此方案，须同步更新版本号策略文档说明迭代编号与 semver 的对应关系。

版本号策略文档（`docs/guide/versioning.md`）须在本迭代完成，说明：semver 规则（何时 bump major/minor/patch）、多 crate 版本是否同步 bump、与迭代编号的关系。

### 发布范围

7 个 workspace crate 的发布策略：

| Crate | 发布目标 | 说明 |
|-------|---------|------|
| agent-runtime-model | crates.io | 公共类型，无 workspace 内部依赖 |
| agent-runtime-core | crates.io | 核心 runtime，依赖 model |
| agent-runtime-providers | crates.io | LLM adapters，依赖 model |
| agent-runtime-aigc-providers | crates.io | AIGC gateway，依赖 model |
| agent-runtime-asr-providers | crates.io | ASR gateway，零 workspace 内部依赖 |
| agent-runtime-py | **不发布 crates.io** | 通过 PyPI/maturin 分发；`publish = false` |
| agent-runtime-node | **不发布 crates.io** | 通过 npm 分发；`publish = false` |

Binding crate 不发布 crates.io 原因：PyO3 / napi-rs 绑定需要配套的 Python/Node 打包工具链（maturin / napi-cli），在 crates.io 上没有可用的消费路径。

### crate 发布顺序

crates.io 要求依赖已发布，发布须按顺序执行：

```
1. agent-runtime-model          # 无 workspace 内部依赖
2. agent-runtime-asr-providers  # 无 workspace 内部依赖（可与 1 并行）
3. agent-runtime-providers      # 依赖 model
4. agent-runtime-aigc-providers # 依赖 model
5. agent-runtime-core           # 依赖 model
```

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

### Cargo 元数据

每个发布 crate 的 `[package]` 须补充以下字段：

```toml
description = "..."            # 一句话描述，≤ 80 字符
license = "MIT OR Apache-2.0"
repository = "https://github.com/jianantian/orchest"
readme = "README.md"           # 指向 crate 自己的 README，或根目录 README
categories = [...]             # 参见 crates.io/category_slugs，≤ 5 个
keywords = [...]               # ≤ 5 个词
```

各 crate 建议值：

| Crate | description | categories |
|-------|-------------|------------|
| agent-runtime-model | Core types for the Orchest agent runtime | `["asynchronous", "api-bindings"]` |
| agent-runtime-core | Agent runtime: run loop, tools, hooks, session, events | `["asynchronous", "api-bindings"]` |
| agent-runtime-providers | LLM provider adapters for the Orchest runtime | `["asynchronous", "api-bindings"]` |
| agent-runtime-aigc-providers | AIGC provider gateway for the Orchest runtime | `["asynchronous", "api-bindings", "multimedia"]` |
| agent-runtime-asr-providers | ASR provider gateway for the Orchest runtime | `["asynchronous", "api-bindings", "multimedia"]` |

Binding crate 须在 `[package]` 中加 `publish = false`。

### 入门教程

`docs/guide/quickstart.md`：从零到一个可运行的 agent，覆盖：

1. 添加 Cargo 依赖（带版本号）
2. 配置 provider（Anthropic 为默认示例，含 env var 用法）
3. 注册 tool（用最简形式展示 tool call 流程）
4. 启动 run + 监听事件（`RunHandle::subscribe_events`）
5. 等待 run 完成，处理结果

**可编译验证**：quickstart 的代码示例须与 `examples/rust/basic_agent_run.rs` 对应（可直接引用或保持一致），CI 通过 `cargo build --example basic_agent_run` 验证。

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
- 安装（`uvx maturin develop` 开发模式 / 未来 pip 安装方式）
- 基础用法（`Agent` 构造、`run()` 调用、event loop 迭代）
- Tool 注册（Python 函数转 tool 的方式）
- 常用 event 类型速查表

`docs/guide/sdk-typescript.md`：
- 安装（`npm install && npm run build:native`）
- 基础用法（`Agent` 构造、`runSync()` / `run()` 调用）
- Tool 注册
- 类型声明说明

SDK 文档的代码示例与 README.md Quick start 保持一致，README 提供最短片段，sdk-*.md 提供完整可运行版本。

### CHANGELOG

`CHANGELOG.md`（根目录），格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)：

```markdown
# Changelog

## [Unreleased]

## [0.9.2] - 2026-xx-xx
### Added
- ...
### Changed
- ...

## [0.9.1] - ...
...
```

- 从 git log 归纳各迭代核心变更，每个迭代一个版本块
- 覆盖 v0.1 ~ v0.9.2（可适当合并早期小版本）
- 后续手动维护；release workflow 不自动写入 CHANGELOG

### Release Workflow

`.github/workflows/release.yml`，触发条件：`push: tags: ['v*.*.*']`

```yaml
jobs:
  release:
    steps:
      - cargo doc --workspace --no-deps      # doc check，无 warning 才继续
      - cargo test --workspace               # 全量测试
      - cargo clippy --workspace -- -D warnings
      - cargo publish -p agent-runtime-model
      - sleep 10                             # 等待 crates.io index 就绪
      - cargo publish -p agent-runtime-asr-providers
      - sleep 10
      - cargo publish -p agent-runtime-providers
      - sleep 10
      - cargo publish -p agent-runtime-aigc-providers
      - sleep 10
      - cargo publish -p agent-runtime-core
      - gh release create $TAG --notes-from-tag
```

**Secret 依赖**：`CARGO_REGISTRY_TOKEN`（crates.io API token），须在 repo settings → secrets 配置（文档说明，workflow 通过 `${{ secrets.CARGO_REGISTRY_TOKEN }}` 引用，不 hardcode）。

## 不在范围内

- 完整的 book-style 文档站（mdBook / Docusaurus）——初版用 rustdoc + markdown guide
- 视频教程
- 多语言翻译
- Python / Node binding 的 PyPI / npm 发布流程
- v0.9.2 之前历史版本的 crates.io 补发
- 版本号策略以外的版本管理自动化（自动 bump、自动生成 tag 等）

## 依赖

- v0.9 Supervised Delegation 已完成（steering / SD 示例需要 v0.9 API）

## 测试策略

| 验证项 | 方法 |
|--------|------|
| cargo doc 无 warning | CI 中 `cargo doc --workspace --no-deps 2>&1 \| grep warning` 返回空 |
| quickstart 可编译 | CI 中 `cargo build --example basic_agent_run` 通过 |
| Cargo 元数据完整 | `cargo publish --dry-run -p <crate>` 对 5 个发布 crate 均通过 |
| examples 全部可编译 | CI 中 `cargo build --examples` 通过 |
| release workflow | 用 dry-run tag 在 feature branch 触发，验证各步骤可达 |

release workflow 的 cargo publish 步骤在 CI 验证阶段用 `--dry-run` 替代实际发布；正式触发只在 tag push 后由维护者手动执行。

## Issue Breakdown

| Issue | 标题 | 依赖 | 范围 |
|-------|------|------|------|
| 001 | cargo-doc 清理 | — | 修复 agent_ref.rs rustdoc warning；补 aigc-providers / asr-providers / node / py 四个 crate 的 `//!` module-level doc |
| 002 | Cargo 元数据 + license | — | 5 个发布 crate 的 description / license / repository / readme / categories / keywords；`LICENSE-MIT` / `LICENSE-APACHE`；binding crate `publish = false`；README License section 更新；版本号决策 + 版本号策略文档 |
| 003 | basic_agent_run 示例 | — | 新增 `examples/rust/basic_agent_run.rs`；注册到 `agent-runtime-core/Cargo.toml [[example]]` |
| 004 | 入门教程 | 003 | `docs/guide/quickstart.md`，代码示例与 basic_agent_run 对应 |
| 005 | SDK 文档 | — | `docs/guide/sdk-python.md`、`docs/guide/sdk-typescript.md` |
| 006 | CHANGELOG | — | 从 git log 归纳生成 `CHANGELOG.md` 初版，覆盖 v0.1–v0.9.2 |
| 007 | Release CI | 001, 002, 006 | `.github/workflows/release.yml`；dry-run 触发验证通过 |

**依赖图：**

```
001 (cargo-doc) ────────────────────────────┐
002 (cargo-meta + license) ─────────────────┤
003 (basic example) ──► 004 (quickstart)    ├──► 007 (release-ci)
005 (sdk-docs)                              │
006 (changelog) ────────────────────────────┘
```

001 / 002 / 003 / 005 / 006 相互独立，可并行开发。004 依赖 003。007 依赖 001 + 002 + 006（doc clean + metadata + changelog 是发布 CI 的前提）。

## 验收标准

### API 文档
- [ ] `cargo doc --workspace --no-deps` 无 warning
- [ ] `agent-runtime-aigc-providers` / `agent-runtime-asr-providers` / `agent-runtime-node` / `agent-runtime-py` 四个 crate 的 `lib.rs` 有 `//!` module-level doc

### License + 元数据
- [ ] 根目录有 `LICENSE-MIT` 和 `LICENSE-APACHE`
- [ ] `agent-runtime-model` / `core` / `providers` / `aigc-providers` / `asr-providers` 的 Cargo.toml 有 description / license / repository / readme
- [ ] `agent-runtime-py` 和 `agent-runtime-node` 标记 `publish = false`
- [ ] `docs/guide/versioning.md` 存在，含 semver 策略和迭代编号对应关系
- [ ] `cargo publish --dry-run -p agent-runtime-model` 通过
- [ ] `cargo publish --dry-run -p agent-runtime-core` 通过
- [ ] `cargo publish --dry-run -p agent-runtime-providers` 通过
- [ ] `cargo publish --dry-run -p agent-runtime-aigc-providers` 通过
- [ ] `cargo publish --dry-run -p agent-runtime-asr-providers` 通过

### 示例 + 文档
- [ ] `examples/rust/basic_agent_run.rs` 存在，`cargo build --example basic_agent_run` 通过
- [ ] `docs/guide/quickstart.md` 存在，代码示例与 basic_agent_run 对应
- [ ] `docs/guide/sdk-python.md` 存在
- [ ] `docs/guide/sdk-typescript.md` 存在
- [ ] `examples/rust/` 覆盖上表全部 9 个场景

### 发布基础设施
- [ ] `CHANGELOG.md` 存在，含 v0.1–v0.9.2 所有版本块
- [ ] `.github/workflows/release.yml` 存在，tag push 触发，dry-run 验证通过
- [ ] README.md License section 更新为正式许可声明

### CI 基线
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `cargo build --examples` 全部通过
