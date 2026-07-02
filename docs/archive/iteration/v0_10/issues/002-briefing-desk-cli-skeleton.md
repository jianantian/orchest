# 002 · Briefing Desk CLI skeleton

## Background

The demo needs to be executable as a product before runtime-specific behavior is added. A thin CLI skeleton gives later issues a stable command surface and smoke-test harness.

## Goal

Create the `examples/demo/briefing-desk` Rust app crate with CLI argument parsing, configuration loading and a deterministic fake-model smoke path.

## Acceptance Criteria

- [ ] `examples/demo/briefing-desk/Cargo.toml` exists and uses workspace path dependencies.
- [ ] `src/main.rs` exposes `run` and `resume` subcommands.
- [ ] `run` accepts `--materials` (directory; may contain `.md`/`.txt`, image files and audio files), `--question`, `--output`, `--session`, `--fake` and `--no-tts` (skip audio synthesis).
- [ ] `resume` accepts `--session`, `--question`, `--output`, `--fake` and `--no-tts`.
- [ ] `src/app.rs` keeps command orchestration separate from tool and media implementations.
- [ ] `tests/smoke.rs` runs the `--fake` path without network credentials; the smoke path includes placeholder calls for the multimedia flow (transcribe → read image → write → synthesize) so the full pipeline is exercised offline.
- [ ] `cargo test -p briefing-desk-demo` passes.
- [ ] The skeleton still uses only public Orchest APIs.

## Notes

The `--fake` flag must be a single switch that simultaneously activates fake model responses, fake ASR transcription and fake TTS synthesis — a single environment variable or CLI flag, not separate per-modality flags. This makes the smoke path unambiguous.

The fake-model path should be deterministic enough for CI. Live provider support can be wired later behind environment variables. Real audio/image fixture files are not needed in this issue; stubs (empty bytes or tiny synthetic files) are enough to exercise the CLI plumbing.
