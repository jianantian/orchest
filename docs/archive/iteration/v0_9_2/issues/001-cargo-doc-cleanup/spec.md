# 001 · cargo-doc 清理 — Spec

## 背景

`cargo doc --workspace --no-deps` 当前产生 1 个 warning，且 4 个 crate 的 `lib.rs` 缺 `//!` module-level 文档。对外发布前，rustdoc 输出须干净，关键 crate 须有 crate-level 概览，让 docs.rs / 本地 rustdoc 的用户一进入就知道每个 crate 是做什么的。

## 目标

`cargo doc --workspace --no-deps` 零 warning；全部 7 个 crate 的 `lib.rs` 都有 `//!` module-level 文档（model / core / providers 已有，补 aigc-providers / asr-providers / node / py 这 4 个）。

## 当前状态（已核实）

实跑 `cargo doc --workspace --no-deps`，仅 1 个 warning：

```
warning: unclosed HTML tag `AgentMsg`
 --> crates/agent-runtime-core/src/run/agent_ref.rs:1:53
```

成因：`agent_ref.rs:1` 的 module doc 为
`//! AgentRef: typed pub(crate) API wrapping ActorRef<AgentMsg>.`
rustdoc 把 `<AgentMsg>` 当作 HTML tag。修复：用反引号包裹 `` `ActorRef<AgentMsg>` ``。

缺 `//!` module-level doc 的 crate：

| Crate | `lib.rs` 现状 |
|-------|--------------|
| `agent-runtime-aigc-providers` | 首行为 `#![allow(clippy::result_large_err)]` |
| `agent-runtime-asr-providers` | 首行为 `#![allow(clippy::result_large_err)]` |
| `agent-runtime-node` | 首行为 `use std::sync::...`（无 crate doc） |
| `agent-runtime-py` | 首行为 `use std::sync::Arc;`（无 crate doc） |

已有 module doc 的 crate（不动）：`agent-runtime-model`、`agent-runtime-core`、`agent-runtime-providers`。

## 范围

1. 修复 `agent_ref.rs:1` 的 rustdoc HTML tag warning
2. 为 4 个 crate 的 `lib.rs` 补 `//!` module-level 文档：一句话定位 + 关键 pub 入口指引（如 aigc-providers 指向 `gateway`、asr-providers 指向 `AsrGateway`/`AsrProvider`、node/py 说明这是 FFI binding crate、业务逻辑在 core）
3. 补关键公共类型的 `///` doc（best effort，针对面向用户的核心 API：`Tool` / `ModelAdapter` / `AgentConfig` / `AgentRun` / `RunHandle` / `RuntimeEvent`、各 gateway 的 trait）。不追求覆盖每个 pub 项

## 不在范围内

- 启用 `#![warn(missing_docs)]` lint（留待后续，避免本迭代引入大面积 doc 债务）
- 为每个 pub fn / pub field 写 doc
- 重写已有的 module doc（model / core）

## 验收标准

- [ ] `cargo doc --workspace --no-deps` 输出零 warning（`2>&1 | grep -i warning` 为空）
- [ ] `agent_ref.rs` 的 module doc 中 `ActorRef<AgentMsg>` 用反引号包裹，warning 消失
- [ ] `agent-runtime-aigc-providers/src/lib.rs` 有 `//!` module-level doc
- [ ] `agent-runtime-asr-providers/src/lib.rs` 有 `//!` module-level doc
- [ ] `agent-runtime-node/src/lib.rs` 有 `//!` module-level doc
- [ ] `agent-runtime-py/src/lib.rs` 有 `//!` module-level doc
- [ ] `cargo clippy --workspace -- -D warnings` 仍全绿（`#![allow]` 属性位置正确）
- [ ] `cargo fmt --check` 通过

## Notes

- `//!` 是 `#![doc = "..."]` 的语法糖，与 `#![allow(...)]` 同为 inner attribute。约定把 `//!` doc 放在文件最顶部，`#![allow(...)]` 紧随其后。
- node / py 是 FFI binding crate，module doc 应明确"业务逻辑在 `agent-runtime-core`，本 crate 只做类型转换和 FFI glue"，与 AGENTS.md 的分层约定一致。
