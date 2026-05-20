# Orchest — Agent 工作指南

## 这是什么

Orchest 是一个**底层 Rust SDK**，为开发者提供构建 AI agent 应用的运行时核心。它不是一个完整的 agent 产品，而是其他 agent 产品的发动机：负责 agent loop、状态管理、事件流、tool 调度、skill 加载。

当前阶段：**纯文档，无实现代码**。所有工作在 `docs/` 目录下进行。代码引入后，`crates/`、`examples/`、`skills/` 将按 Rust 项目规范一节的结构组织。

---

## 术语边界（最重要，不得混淆）

这三个概念在 Anthropic 生态中常被混淆，Orchest 严格区分：

| 概念 | 定义 | 层次 |
|------|------|------|
| **Tool** | 模型可以调用的最小原子能力单元，包含名称、schema、execute 实现 | 能力层 |
| **MCP** | Tool 提供方的传输协议，不是特殊的 tool 类型 | 协议层 |
| **Skill** | 通过文件系统组织的过程性知识包（SKILL.md + 可选脚本），按渐进式披露暴露给 agent | 知识层 |

**MCP 不是 tool 的替代品**，它是把 tool 的发现和执行从应用代码中解耦出来的协议层。通过 MCP 接入的 tool 和直接注册的 in-process tool 在 runtime 内部通过同一个 `Tool` trait 统一对待。

**Skill 的核心不是 tool 的集合**，而是 how-to 知识。很多 skill 完全由 markdown 构成，调用的是当前会话已有的 tool。

遇到任何关于这三者边界的疑问，以 `docs/polaris/concept-boundaries.md` 为准。

---

## 文档地图

```
docs/
├── overview.md              # 产品定位、核心概念、设计哲学（面向外部读者）
├── spec.md                  # 原始技术设计参考（已部分过时，见下方权威规则）
├── polaris/
│   ├── concept-boundaries.md  # Tool/MCP/Skill 边界定义（权威）
│   ├── design-principles.md   # 设计原则和决策参考问题
│   └── non-goals.md           # 硬性边界 + 无沙箱环境的最低安全建议
├── iteration/
│   ├── v0_1/                  # 最小可用：Rust core + 双语言 SDK
│   ├── v0_2/                  # MCP 集成 + OpenAI adapter + context compaction
│   └── v0_3/                  # 生产完整度：skill 依赖 + code exec + sub-agent + 沙箱架构
└── research/
    └── claw-landscape.md      # 7 个同类产品的架构研究（提炼 SDK 层可参考的设计）
```

### 权威规则（重要）

**迭代文档 > spec.md**。`docs/spec.md` 是原始设计，部分内容已被迭代文档取代。如有冲突，以 `docs/iteration/` 为准。`spec.md` 保留作原始参考，不应在其中做"权威"修改。

每个迭代有两层文档：
- `prd.md`：迭代目标、成功指标、范围、不在范围内
- `issues/*.md`：拆解成可执行的实现单元，含验收标准

---

## 迭代状态

| 迭代 | 状态 | 核心内容 |
|------|------|---------|
| **v0.1** | 文档完成，待实现 | Rust core run loop、skill 加载、异步 job、budget guard、approval gate、Python/TS SDK |
| **v0.2** | 文档完成，待实现 | MCP stdio/HTTP、Tool Search Tool、OpenAI adapter、context compaction、webhook 异步 tool |
| **v0.3** | 文档完成，待实现 | Skill 依赖管理、Code Execution MCP、sub-agent、ScriptExecutor 抽象 + capability 声明 |

---

## 锁定的设计决策（不要试图改变）

以下决策已经过充分讨论，不需要重新论证：

- **Rust 核心 + PyO3/napi-rs**：跨语言 SDK 需要 in-process 嵌入而非 IPC，Rust 是唯一合理选择
- **Skill-first**：完整对齐 Anthropic Agent Skills 开放标准，SKILL.md 格式不得与官方标准不兼容
- **MCP 是传输协议而非 tool 类型**：通过 MCP 接入的 tool 在 runtime 内部通过 `Tool` trait 统一对待
- **极简 core**：runtime 只做"循环 + 状态管理 + 事件流"，所有能力外移到 tool 和 skill
- **流式输出是 v0.1 核心**：不是可选项，`ModelAdapter::stream()` 是主路径
- **v0.1 顺序执行 tool call**：保持 approval gate 简单，并行是 v0.2 优化项
- **沙箱留到 v0.3 之后**：但 v0.3 必须完成 `ScriptExecutor` trait 抽象和 `capabilities` 声明

---

## 做文档变更时的规范

### 新增 issue

1. 放在对应迭代的 `issues/` 目录下，文件名格式：`NNN-slug.md`（三位数字前缀）
2. 必须包含：背景、目标、验收标准（checkbox 列表）、说明（可选）
3. 验收标准要具体可测，不能写"实现 X"，要写"当 Y 时，Z 成立"

### 修改 spec.md

spec.md 里的类型定义（`ToolMetadata`、`ModelStreamChunk`、`RunStatus` 等）是 v0.1 的实现合同。修改时：
- 同步更新受影响的 issue 验收标准
- 在 `## 设计决策记录` 末尾补充决策理由

### 修改 polaris 文档

polaris 文档记录的是**不随迭代变化的约束**。修改要谨慎，改之前先确认这真的是永久边界而不是当前迭代的取舍。

### 不要做的事

- 不要在 spec.md 里直接做"权威"变更而不更新对应 issue
- 不要在 overview.md 里加实现细节（overview 面向外部读者）
- 不要把多频道路由、用户管理、Web UI 等产品层需求带进 SDK 设计
- 不要在没有对应 polaris 依据的情况下新增 Non-Goal

---

## Rust 项目规范

### Workspace 结构

```
Cargo.toml                      # workspace root，不含业务代码
crates/
  agent-runtime-core/           # 纯 Rust 核心，无 FFI
    src/
      lib.rs
      run.rs                    # AgentRun, RunState, run loop
      tool/
        mod.rs                  # Tool trait, ToolRegistry, ToolOutput
        in_process.rs           # FFI callback tool
        skill_bundled.rs        # script tool + async job 协议解析
        async_job.rs            # JobHandle, JobStatus, poll loop
        builtin.rs              # read_file（内置 tool）
        mcp.rs                  # MCP tool（v0.2 加入）
      skill/
        mod.rs                  # SkillManifest, discovery, SKILL.md 解析
        executor.rs             # ScriptExecutor trait + BareSubprocessExecutor
      model/
        mod.rs                  # ModelAdapter trait
        anthropic.rs
        openai.rs               # v0.2 加入
        streaming.rs            # ModelStreamChunk 公共逻辑
      events.rs                 # RuntimeEvent enum
      budget.rs                 # BudgetGuard, BudgetConfig, BudgetUsage
  agent-runtime-py/             # PyO3 binding，不含核心逻辑
    src/lib.rs
  agent-runtime-node/           # napi-rs binding，不含核心逻辑
    src/lib.rs
examples/
skills/                         # 示例 skill
```

**原则**：核心逻辑只在 `agent-runtime-core`，binding crate 只做类型转换和 FFI 胶水，不含业务判断。

### 依赖规范

**已确定的核心依赖（不要替换）：**

| 依赖 | 用途 | Feature |
|------|------|---------|
| `tokio` | 异步运行时 | `full` |
| `serde` + `serde_json` | 序列化 | `derive` |
| `async-trait` | 异步 trait 对象 | — |
| `uuid` | RunId | `v4`, `serde` |
| `pyo3` | Python binding | `extension-module` |
| `napi` + `napi-derive` | Node.js binding | — |

**新增依赖的原则：**
- 优先 std + tokio，避免引入 actor framework（已锁定决策）
- 错误处理用 `thiserror`（library crate），不用 `anyhow`（application crate）
- 新依赖需要在 PR body 或 commit body 里说明理由和备选方案

### 错误处理

- **每个模块定义自己的 `XxxError`**，用 `thiserror` derive：`ToolError`、`ModelError`、`SkillError`、`BudgetError`
- **library code 禁止 `unwrap()` 和 `expect()`**，除非在 `#[cfg(test)]` 块或有充分注释的不变量保证（如 mutex poison）
- FFI 边界（PyO3/napi）统一把内部 error 转换为对应语言的 exception/Error，不透传 Rust error 类型

### Trait 与可见性

- `pub trait` 只用于公开 API（`Tool`、`ModelAdapter`、`ScriptExecutor`）；内部扩展点用 `pub(crate) trait`
- 实现类型默认 `pub(crate)`，只有需要在 binding crate 里构造的类型才 `pub`
- **不要为了省事把整个模块 `pub use *`**，明确 re-export 哪些类型

### 异步规范

- run loop 跑在 `tokio::spawn` 的 task 上；event channel 用 `tokio::sync::mpsc`；approval gate 用 `tokio::sync::oneshot`
- **trait 方法用 `async-trait`**，不用 `-> impl Future`（与 PyO3/napi FFI 不兼容）
- blocking 操作（文件 I/O、子进程等）用 `tokio::task::spawn_blocking` 包裹，不在 async context 里直接阻塞

### 序列化规范

- 跨 FFI 传递的类型必须实现 `Serialize + Deserialize`
- `JobHandle.poll` 闭包**不可序列化**，`RunState` 序列化时跳过该字段（`#[serde(skip)]`），文档注释说明跨进程恢复的限制
- `JsonSchema` 在 v0.1 用 `serde_json::Value` 类型别名，不引入 jsonschema crate

### unsafe 规范

- **`agent-runtime-core` 禁止 `unsafe`**
- PyO3 和 napi-rs 的 binding crate 因 FFI 需要，允许 `unsafe`，但必须：
  - 每处 `unsafe` 块都有注释说明 safety invariant
  - 不在 `unsafe` 块里做业务逻辑，只做类型转换

### 测试规范

- **单元测试**：`#[cfg(test)]` 放在对应文件末尾，mock 用 struct 实现 trait（不引入 mockall 等框架）
- **集成测试**：`tests/` 放在 workspace root，每个场景一个文件，文件名描述场景（`tool_async_job.rs`、`skill_loading.rs`）
- **测试辅助 struct** 命名加 `Fake` 前缀（`FakeModelAdapter`、`FakeScriptExecutor`），放在 `#[cfg(test)]` 模块或 `tests/helpers/` 下
- CI 必须通过：`cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`

### 命名规范

| 场景 | 规范 | 示例 |
|------|------|------|
| 类型 / trait | `PascalCase` | `ToolMetadata`, `ModelAdapter` |
| 方法 / 变量 | `snake_case` | `execute()`, `run_id` |
| 常量 | `SCREAMING_SNAKE_CASE` | `MAX_POLL_RETRIES` |
| 模块文件 | `snake_case` | `async_job.rs`, `skill_bundled.rs` |
| 错误类型 | `XxxError` | `ToolError`, `ModelError` |
| 测试辅助 | `FakeXxx` | `FakeModelAdapter` |
| Feature flag | `kebab-case` | `mcp`, `openai` |

### 代码组织原则

- 每个文件专注一个主类型或 trait；超过 400 行考虑拆分
- `mod.rs` 只做 re-export 和模块声明，逻辑放在子文件
- run loop 的核心状态机逻辑集中在 `run.rs`，不要把 loop 的逻辑分散到各个 tool/model 模块里

---

## Commit 规范

前缀：`docs:`（文档）、`feat:`（功能，实现阶段）、`fix:`（修复）、`refactor:`（重构）

Subject 示例：
- `docs: add v0.2 issue for webhook async tool`
- `docs: clarify ScriptExecutor trait in spec`
- `docs: rename v1.0 to v0.3, add sandbox-ready architecture`

Subject 长度控制在 72 字符以内。Body 说明变更原因和影响范围（特别是跨多个文件的连锁变更）。

---

## 常用检索命令

```bash
# 在设计文档中搜索关键词
rg "术语或类型名" docs/

# 列出所有文档文件
find docs -maxdepth 4 -name "*.md" | sort

# 查看所有未完成的验收标准
rg "\- \[ \]" docs/iteration/

# 确认没有遗漏的 v1.0 引用（应为空）
rg "v1\.0|v1_0" docs/
```
