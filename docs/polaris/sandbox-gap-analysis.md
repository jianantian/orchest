# Sandbox & Path Safety — Gap Analysis & Spec

**Status**: draft — documents current state, gaps, and incremental design.
**Scope**: `ReadFileTool`, `WriteFileTool`, `ExecutePythonTool`, `ExecuteJavaScriptTool`, `SkillBundledTool`, `BareSubprocessExecutor`.
**Non-goal**: full container/VM/firejail sandbox (deferred per `docs/polaris/non-goals.md`).

---

## 1. Current State — Per-Tool Protection Matrix

| Tool | Path restriction | Env isolation | Process isolation | Approval gate |
|------|-----------------|---------------|-------------------|---------------|
| `ReadFileTool` | **none** — arbitrary path | N/A | N/A | no |
| `WriteFileTool` | **none** — arbitrary path | N/A | N/A | yes (`side_effect: true`) |
| `ExecutePythonTool` | **none** — cwd = agent cwd | **inherits full parent env** | separate process, kill-on-timeout | no |
| `ExecuteJavaScriptTool` | **none** — same | **inherits full parent env** | separate process, kill-on-timeout | no |
| `SkillBundledTool` | `starts_with(skill_dir)` — **yes** | `.env_clear()` + whitelist — **yes** | separate process, kill-on-timeout | metadata-driven |
| `BareSubprocessExecutor` | work_dir = tempdir — **yes** | `.env_clear().envs(&ctx.env)` — **yes** | separate process, kill-on-timeout | N/A |

**Key asymmetry**: `SkillBundledTool` has both path and env protection. The builtin tools (`read_file`, `write_file`, `execute_python`, `execute_javascript`) have neither. This is not a sandbox question — it's a consistency question within the same runtime.

---

## 2. Gap Analysis

### 2.1 `ReadFileTool` — arbitrary file read

**Current behavior**: model can read any path the agent process has access to.

```
User: "read ../.env"
Agent: calls read_file("../.env")  ← succeeds, leaks secrets
```

**Risk**: API keys, credentials, config files, source code, SSH keys.

**Why this isn't a sandbox problem**: `SkillBundledTool` already solves the equivalent problem for skill scripts with a 15-line check (`script_resolved.starts_with(&skill_dir_resolved)`). The same approach — an `allowed_roots` config — would close this gap without any OS-level sandboxing.

### 2.2 `WriteFileTool` — arbitrary file write

**Current behavior**: model can write to any path (with `requires_approval: true` by default).

```
User: "write the report to /tmp/report.md"
Agent: calls write_file("/tmp/report.md", ...)  ← approval-gated, but path unrestricted
```

**Risk**: overwriting config files, dropping payloads into startup directories, `.bashrc` injection.

**Mitigation today**: `requires_approval` gate. But approval only gates the *call*, not the *path*. A user who approves "write /tmp/report.md" doesn't realize the model could ask for "/etc/cron.d/payload" next turn.

### 2.3 `ExecutePythonTool` / `ExecuteJavaScriptTool` — env inheritance

**Current behavior**: subprocess inherits full parent environment.

```
parent env: AWS_ACCESS_KEY_ID=..., DATABASE_URL=..., GITHUB_TOKEN=...
child process: has all of them
```

**Contrast with `SkillBundledTool`**: `BareSubprocessExecutor` does `.env_clear().envs(&ctx.env)`, passing only variables declared in the skill's `capabilities.env` whitelist. The code execution tools should match this pattern — pass a curated subset (e.g. `PATH`, `HOME`, plus a user-configured whitelist).

### 2.4 No filesystem guard for code execution tools

Python and JS subprocesses run with the agent's cwd. They can `os.listdir("..")`, `open("/etc/passwd")`, `subprocess.run("curl ...")`. `SkillBundledTool` mitigates this with `work_dir = tempdir` — the script can't even see the agent's working directory. The code execution tools have no equivalent.

---

## 3. Design — Incremental (v0.7)

Full sandboxing (firejail, bubblewrap, seccomp, Landlock) is deferred. The following changes are **application-level guardrails** that don't require OS support:

### 3.1 `allowed_roots` for `ReadFileTool` and `WriteFileTool`

```rust
pub struct ReadFileTool {
    allowed_roots: Vec<PathBuf>,  // empty = unrestricted (backward compat)
    // ...
}
```

- Default: `vec![]` → no restriction (backward compatible).
- When configured: resolve `path` to canonical form, check `starts_with(any allowed_root)`.
- Reject with `ToolError { code: "PATH_NOT_ALLOWED" }` before any I/O.
- Same mechanism for `WriteFileTool`.

Configuration via `AgentConfig`:

```rust
pub struct RuntimeConfig {
    pub file_read_roots: Vec<PathBuf>,   // default empty
    pub file_write_roots: Vec<PathBuf>,  // default empty
}
```

### 3.2 Environment whitelist for `ExecutePythonTool` / `ExecuteJavaScriptTool`

```rust
pub struct RuntimeConfig {
    pub code_exec_env_allow: Vec<String>,  // env vars to pass to code exec subprocess
}
```

When non-empty: `.env_clear()` then pass only listed vars + `PATH`. When empty: inherit parent env (backward compat). Aligns with `BareSubprocessExecutor` pattern.

### 3.3 `work_dir` isolation for code execution tools

- Default: `tempfile::tempdir()` per execution (same as `SkillBundledTool::spawn_script`).
- This prevents the subprocess from even seeing the agent's cwd.
- Cost: one `mkdir` + `rmdir` per `execute_python` call. Negligible.

### 3.4 Canonical path resolution everywhere

`ReadFileTool`, `WriteFileTool`, and `SkillBundledTool::new()` already use `canonicalize()`. Ensure `WriteFileTool` also canonicalizes before writing (it currently doesn't — it uses the raw path string to create parent dirs). Canonicalize the *parent* before `create_dir_all`.

---

## 4. Design — Full Sandbox (v0.8+)

Deferred per `docs/polaris/non-goals.md`. When we revisit:

| Layer | Mechanism | Scope |
|-------|-----------|-------|
| Filesystem | `Landlock` (Linux 5.13+) / `sandbox-exec` (macOS) | `ReadFileTool`, `WriteFileTool`, code exec |
| Network | seccomp-bpf deny-by-default | code exec |
| Process | PID namespace or `unshare(CLONE_NEWPID)` | code exec |
| Memory | cgroups v2 `memory.max` | code exec (`capabilities.max_memory_mb`) |
| Time | `setrlimit(RLIMIT_CPU)` | code exec (`capabilities.timeout`) |

All of these require OS-specific code paths and fallback detection. The `ScriptExecutor` trait abstraction in `skill/executor.rs` is the injection point.

---

## 5. Acceptance Criteria

### v0.7

- [ ] `ReadFileTool` rejects paths outside `file_read_roots` when configured
- [ ] `WriteFileTool` rejects paths outside `file_write_roots` when configured, canonicalizes parent before `create_dir_all`
- [ ] `ExecutePythonTool` and `ExecuteJavaScriptTool` use `.env_clear()` when `code_exec_env_allow` is non-empty
- [ ] Code execution tools create a tempdir `work_dir` per invocation
- [ ] All three tools emit `ToolCallFailed { error: "path not allowed" }` or equivalent structured error, not a raw OS error
- [ ] Existing tests pass with default (unrestricted) config
- [ ] New tests: path rejection, env isolation, tempdir isolation

### v0.8+ (full sandbox — deferred)

- [ ] `ScriptExecutor` trait gains `sandbox: SandboxConfig` parameter
- [ ] `SandboxedExecutor` wraps `BareSubprocessExecutor` with OS sandbox
- [ ] Graceful fallback on unsupported kernels (log warning, run unsandboxed)
- [ ] `capabilities.network`, `capabilities.filesystem_read/write` honored at OS level
