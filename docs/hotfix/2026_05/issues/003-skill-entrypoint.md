# 003 · Wire Skill Loading Into SDK/Runtime Entrypoints

## Background

`SkillScanner`, `SkillBundledTool`, `SkillEnvManager`, and `CapabilityValidator` exist, but SDK entrypoints currently only store `skills_dir` and do not scan or register skill bundled tools. As a result, `skills_dir` is a dead field, `SkillMissingCapabilities` is not observable from normal runs, and `SkillContentRead` telemetry requires manual test setup.

## Goal

Make `skills_dir` functional from Python and TypeScript SDK runs, and keep the skill-first runtime contract intact.

## Acceptance Criteria

**Skill scanning and registration:**
- [ ] Python SDK run scans `skills_dir` when provided
- [ ] TypeScript SDK run scans `skillsDir` when provided
- [ ] Each allowed `SkillManifest.bundled_tools` entry is registered as a `SkillBundledTool`
- [ ] Skill dependencies and capabilities from the manifest are passed into `SkillBundledTool::new_with_options`
- [ ] Duplicate tool names produce a clear run failure or SDK error before model execution begins

**Skill telemetry:**
- [ ] Builtin `read_file` is registered by SDK/runtime when skill support is enabled
- [ ] Each allowed skill `SKILL.md` path is registered with `ReadFileTool::register_skill`
- [ ] Reading an allowed skill `SKILL.md` emits `SkillContentRead`

**Capability warnings:**
- [ ] A skill with `scripts/` and no `capabilities` emits `SkillMissingCapabilities { skill_name }`
- [ ] The warning does not block execution
- [ ] A skill with declared `capabilities` does not emit the warning

**SDK-visible behavior:**
- [ ] A Python example or test calls a bundled skill tool through `agent.run(...)`
- [ ] A TypeScript example or test calls a bundled skill tool through `agent.run(...)`

## Notes

Do not load full `SKILL.md` bodies into the model automatically. The existing design remains progressive disclosure: expose a compact skill/tool list, and let the agent read `SKILL.md` via `read_file` when needed.
