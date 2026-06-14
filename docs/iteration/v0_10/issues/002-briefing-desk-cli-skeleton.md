# 002 · Briefing Desk CLI skeleton

## Background

The demo needs to be executable as a product before runtime-specific behavior is added. A thin CLI skeleton gives later issues a stable command surface and smoke-test harness.

## Goal

Create the `examples/demo/briefing-desk` Rust app crate with CLI argument parsing, configuration loading and a deterministic fake-model smoke path.

## Acceptance Criteria

- [ ] `examples/demo/briefing-desk/Cargo.toml` exists and uses workspace path dependencies.
- [ ] `src/main.rs` exposes `run` and `resume` subcommands.
- [ ] `run` accepts `--materials`, `--question`, `--output`, `--session` and `--fake-model`.
- [ ] `resume` accepts `--session`, `--question`, `--output` and `--fake-model`.
- [ ] `src/app.rs` keeps command orchestration separate from tool implementations.
- [ ] `tests/smoke.rs` runs the fake-model path without network credentials.
- [ ] `cargo test -p briefing-desk-demo` passes.
- [ ] The skeleton still uses only public Orchest APIs.

## Notes

The fake-model path should be deterministic enough for CI. Live provider support can be wired later behind environment variables.
