# 008 · 测试覆盖补齐 + 文档修复

## 背景

Review 发现的测试覆盖缺口和文档/类型问题。在前面 7 个 issue 的代码改动稳定后统一补齐。

## 测试覆盖缺口

| 模块 | 当前状态 | 需要补充 |
|------|---------|---------|
| `run/compaction.rs` | 无测试 | compact 触发条件、消息保留数量、空消息边界 |
| `run/webhook.rs` | 无测试 | webhook 回调解析、invalid payload、timeout |
| `run/tool_exec.rs` | 间接覆盖 | 超时路径、webhook 路径独立测试 |
| `tool/mcp.rs` | 基本 | HTTP 错误码处理、连接失败重试边界 |
| `agent-runtime-py/src/lib.rs` | 无 `#[cfg(test)]` | FFI 调用 smoke test |
| `agent-runtime-node/src/lib.rs` | 无 `#[cfg(test)]` | FFI 调用 smoke test |
| Python SDK | 5 基础测试 | tool 注册、事件消费、异常类型 |
| `e2e_validation.rs` | 主要事件 | compaction/webhook/sub-agent budget 继承场景 |

每个模块至少补充 2-3 个测试覆盖核心路径和关键边界。

## 文档与类型修复（T1–T6, T8–T9）

| ID | 问题 | 修复 |
|----|------|------|
| T1 | 多数 `.rs` 缺 `//!` module doc | 每个 `mod.rs` 和独立模块文件添加一行 `//!` 注释说明职责 |
| T2 | `code_execution_enabled` 缺安全文档 | 在 `AgentConfig` 的 `code_execution_enabled` 字段添加 doc comment，说明安全边界（非沙箱、不适合不可信代码） |
| T3 | Python `JsonValue: TypeAlias = Any` 太宽 | 改为 `JsonValue = dict[str, Any] | list[Any] | str | int | float | bool | None`；同步更新 `.pyi` |
| T4 | `js/index.js` 是 71B 存根 | 删除或改为正确指向 `index.ts` 编译产物的入口 |
| T5 | `docs/polaris/overview.md` 和 polaris docs 纯中文 | 不修改（项目文档语言是作者选择，不是 bug） |
| T6 | `AgentDelegate` 有 `output_mapper` 无 `input_mapper` | 在 doc comment 中说明 `input_mapper` 省略的设计意图（本轮不新增对外可见功能；如确需添加，留 v0.7） |
| T8 | Pricing 常量硬编码 | 将定价常量集中到 `providers/src/pricing.rs`，每个 adapter 引用而非各自内联 |
| T9 | 测试硬编码 `python3` | 提取为 `const PYTHON_BIN: &str` 或读取 `PYTHON_BIN` 环境变量 |

T5 标注"不修改"——中文文档不是 bug。

## 验收标准

- [ ] `run/compaction.rs` 至少 3 个测试（触发、保留、边界）
- [ ] `run/webhook.rs` 至少 2 个测试（正常回调、异常 payload）
- [ ] `tool/mcp.rs` 增加 HTTP 错误处理测试
- [ ] binding crates 各有至少 1 个 `#[cfg(test)]` smoke test
- [ ] Python SDK 增加 tool 注册和事件消费测试
- [ ] T1：所有非测试 `.rs` 文件有 `//!` module doc
- [ ] T2：`code_execution_enabled` 有安全边界 doc comment
- [ ] T3：Python `JsonValue` 类型收窄，`.pyi` 同步
- [ ] T4：`js/index.js` 问题解决
- [ ] T6：`AgentDelegate` 添加 doc comment 说明 `input_mapper` 省略原因（不新增字段）
- [ ] T8：定价常量集中管理
- [ ] T9：测试 python 路径可配置
- [ ] `cargo test --workspace` 全绿
