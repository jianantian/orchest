# 004 · CI 和 Config 修复 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Fix small CI/config correctness issues that currently let stale paths and over-broad dependencies slip through.

**Architecture:** Make targeted edits only: `.gitignore`, one lint script, and `agent-runtime-core` tokio features. Do not restructure CI or dependency layout.

**Tech Stack:** Bash, Cargo, Tokio feature flags.

---

## 要读的现有代码

- `.gitignore`
- `scripts/check-ts-event-wire-naming.sh`
- `crates/agent-runtime-core/Cargo.toml`
- `crates/agent-runtime-core/src/**`

## 文件改动

- Modify: `.gitignore`
- Modify: `scripts/check-ts-event-wire-naming.sh`
- Modify: `crates/agent-runtime-core/Cargo.toml`

## 步骤

### 1. `.gitignore` 修正 macOS 文件名

- [ ] Replace `.DS_STORE` with `.DS_Store`.
- [ ] Verify:

```bash
rg "\\.DS_STORE" .gitignore
rg "\\.DS_Store" .gitignore
```

Expected: first command no output, second command one match.

### 2. TS wire naming script 修复 scan targets

- [ ] Change target from stale iteration path to archive root:

```bash
targets=(
  "js"
  "examples"
  "docs/archive/iteration"
)
```

- [ ] Add target existence guard immediately after the `targets` array:

```bash
for target in "${targets[@]}"; do
  if [[ ! -e "$target" ]]; then
    echo "error: scan target '$target' does not exist" >&2
    exit 1
  fi
done
```

- [ ] Run:

```bash
./scripts/check-ts-event-wire-naming.sh
```

Expected: `RuntimeEvent wire naming check passed.`

### 3. Core tokio features 收窄

- [ ] Audit tokio usage:

```bash
rg "tokio::|#\\[tokio::" crates/agent-runtime-core/src crates/agent-runtime-core/tests
```

- [ ] Replace core dependency with the minimal feature set expected by current usage:

```toml
tokio = { version = "1", features = ["rt", "rt-multi-thread", "sync", "time", "macros", "io-util", "process", "fs", "net"] }
```

- [ ] Do not remove `fs`, `net`, or `rt-multi-thread`: current core code uses `tokio::fs`, `tokio::net`, and multi-thread tokio tests.
- [ ] Run:

```bash
cargo test -p agent-runtime-core
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

### 4. 验证

```bash
rg "features = \\[\"full\"\\]" crates/agent-runtime-core/Cargo.toml
./scripts/check-ts-event-wire-naming.sh
cargo test --workspace
```

Expected: `rg` has no output; script and tests exit 0.
