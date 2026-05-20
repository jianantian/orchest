# v0.1 E2E Validation Notes

This checklist records operator-visible validation notes that must be called out when running the v0.1 demos or E2E suite.

## Known Limitations

### `read_file` path boundary

v0.1 intentionally keeps builtin `read_file` permissive: it reads any path the host process can read, and it does not enforce a path allowlist. This is an accepted v0.1 limitation, not a complete security boundary.

Validation must make the boundary visible:

- `SkillContentRead` is emitted when `read_file` reads a registered skill `SKILL.md`.
- Non-skill file reads are allowed in v0.1 and do not emit `SkillContentRead`.
- Operators should treat this as a known risk when reviewing demo output, E2E logs, or event streams.

Relevant rationale:

- [Polaris non-goals: v0.1 sandbox limitations](../../polaris/non-goals.md#v01-明确不做留给-v02)
- [Polaris non-goals: minimum guidance for sandboxless environments](../../polaris/non-goals.md#无沙箱环境的最低运营建议)
- [Issue 011: Builtin read_file Tool](./issues/011-builtin-read-file.md)
