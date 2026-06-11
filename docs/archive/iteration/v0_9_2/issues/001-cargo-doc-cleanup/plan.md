# 001 · cargo-doc 清理 — 实现计划

## 要读的现有代码

- `crates/agent-runtime-core/src/run/agent_ref.rs:1` — 待修的 module doc
- `crates/agent-runtime-model/src/lib.rs:1` — 已有 module doc，参考风格
- `crates/agent-runtime-core/src/lib.rs:1` — 已有 module doc，参考风格
- `crates/agent-runtime-aigc-providers/src/lib.rs` — 看 pub 入口（gateway / types / providers）
- `crates/agent-runtime-asr-providers/src/lib.rs` — 看 pub re-export（AsrGateway / AsrProvider / ...）
- `crates/agent-runtime-node/src/lib.rs`、`crates/agent-runtime-py/src/lib.rs` — 确认首行结构

## 步骤

### 1. 修 agent_ref.rs warning

`crates/agent-runtime-core/src/run/agent_ref.rs:1`：

```rust
//! AgentRef: typed pub(crate) API wrapping `ActorRef<AgentMsg>`.
```

### 2. 补 aigc-providers / asr-providers 的 `//!`

两者首行是 `#![allow(clippy::result_large_err)]`。在其**上方**插入 `//!`：

```rust
//! AIGC provider gateway for the Orchest runtime.
//!
//! Unified abstraction over image-generation providers ... 入口见 [`gateway`]。
#![allow(clippy::result_large_err)]
```

asr-providers 同理，指向 `AsrGateway` / `AsrProvider` / `start_stream`。

### 3. 补 node / py 的 `//!`

两者首行是 `use ...`。在文件最顶部插入：

```rust
//! Node.js (napi-rs) bindings for the Orchest agent runtime.
//!
//! This crate only does type conversion and FFI glue; all business logic
//! lives in `agent-runtime-core`.

use std::sync::...;
```

py 同理（PyO3）。

### 4. 补核心 pub 类型 `///`（best effort）

逐个检查并在缺失处补一行 `///`：
- core：`Tool` trait、`AgentConfig`、`AgentRun`、`RunHandle`、`RuntimeEvent`、`ToolRegistry`
- model：`ModelAdapter`（已有 module doc，确认 trait 本身有 `///`）
- aigc / asr：各自的 gateway struct 和 provider trait

只补面向用户的核心 API，不下钻到每个字段。

### 5. 验证

```bash
cargo doc --workspace --no-deps 2>&1 | grep -i warning   # 应为空
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## 关键决策

- **不启用 `#![warn(missing_docs)]`**：会一次性暴露大量缺失，超出本迭代"清理 + 关键概览"的范围。留作后续单独的 doc 债务清偿。
- **`//!` 与 `#![allow]` 顺序**：doc 在前，allow 在后。`cargo fmt` 不会重排，但风格统一。
