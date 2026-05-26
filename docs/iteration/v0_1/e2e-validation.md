# v0.1 E2E Validation Notes

This checklist records operator-visible validation notes that must be called out when running the v0.1 demos or E2E suite.

## Validation Commands

Run the standard Rust checks:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Run the TypeScript-facing RuntimeEvent wire naming guard:

```bash
./scripts/check-ts-event-wire-naming.sh
```

This guard is intentionally narrow: TypeScript API fields may remain idiomatic camelCase, but `RuntimeEvent.type` wire discriminants must stay snake_case.

## Known Limitations

### `read_file` path boundary

v0.1 intentionally keeps builtin `read_file` permissive: it reads any path the host process can read, and it does not enforce a path allowlist. This is an accepted v0.1 limitation, not a complete security boundary.

Validation must make the boundary visible:

- `SkillContentRead` is emitted when `read_file` reads a registered skill `SKILL.md`.
- Non-skill file reads are allowed in v0.1 and do not emit `SkillContentRead`.
- Operators should treat this as a known risk when reviewing demo output, E2E logs, or event streams.

Relevant rationale:

- [v0.1 PRD: out of scope](./prd.md#不在范围内)
- [Issue 011: Builtin read_file Tool](./issues/011-builtin-read-file/spec.md)
