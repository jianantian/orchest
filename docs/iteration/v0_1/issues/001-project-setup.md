# 001 · 项目结构与 Cargo workspace 初始化

## 背景

在写任何业务逻辑之前，先把 Cargo workspace、目录结构、依赖版本确定下来，避免后续每个 issue 都在做结构调整。

## 目标

建立可以编译通过的 workspace 骨架，三个 crate 各自能 `cargo check`。

## 验收标准

- [ ] workspace `Cargo.toml` 包含三个 member：`agent-runtime-core`、`agent-runtime-py`、`agent-runtime-node`
- [ ] `agent-runtime-core` 包含空的 `src/lib.rs`，依赖：`tokio`（full features）、`serde`（derive）、`serde_json`、`async-trait`、`uuid`
- [ ] `agent-runtime-py` 依赖 `pyo3`（features: extension-module）和 `agent-runtime-core`
- [ ] `agent-runtime-node` 依赖 `napi`、`napi-derive` 和 `agent-runtime-core`
- [ ] `cargo check --workspace` 无错误
- [ ] `.gitignore` 覆盖 `target/`、`*.so`、`*.dylib`、`*.dll`、Python `__pycache__`、Node `node_modules`
- [ ] 目录结构与 spec 中"目录结构"章节一致

## 目录结构参考

```
crates/
  agent-runtime-core/src/lib.rs
  agent-runtime-py/src/lib.rs
  agent-runtime-node/src/lib.rs
examples/
skills/
docs/
Cargo.toml          # workspace root
```
