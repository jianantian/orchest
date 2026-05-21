# 001 · Enforce Permission Boundaries

## Background

`AgentConfig.allowed_tools` and `AgentConfig.allowed_skills` are documented as permission boundaries. The current root run path still exposes all registered tools to the model and allows execution by name lookup. `allowed_skills` is not applied during skill registration because skill loading is not yet wired into the run entrypoint.

This is a security contract bug, not a feature gap.

## Goal

Make configured tool and skill restrictions authoritative for root agents and inherited sub-agents.

## Acceptance Criteria

**Tool visibility and execution:**
- [ ] When `AgentConfig.allowed_tools = Some([...])`, the first model call receives only those tool schemas plus any runtime-required control tools explicitly allowed by policy
- [ ] When the model emits a tool call for a tool not in `allowed_tools`, runtime does not execute it and returns a tool result error such as `{"error": "tool not allowed"}`
- [ ] `ToolCallStarted` is not emitted for denied-by-policy tools
- [ ] The behavior is covered by a deterministic core test where the registry contains at least two tools and only one is allowed

**Skill restrictions:**
- [ ] When `allowed_skills = Some([...])`, only those skill manifests are registered from `skills_dir`
- [ ] Bundled tools from disallowed skills are not visible to the model and cannot execute by guessed name
- [ ] `read_file` skill telemetry is registered only for allowed skills
- [ ] Directly registered tools are filtered by `allowed_tools` when it is set
- [ ] Skill bundled tools are first filtered by `allowed_skills`; if `allowed_tools` is also set, the remaining bundled tools are further filtered by tool name

**Sub-agent inheritance:**
- [ ] Sub-agents inherit parent `allowed_tools` and `allowed_skills` by default
- [ ] Sub-agent config may further narrow these lists but cannot expand them
- [ ] A test verifies an attempted sub-agent expansion is ignored or rejected

## Notes

Prefer filtering the registry before building model-visible tool definitions, and still keep an execution-time guard. Visibility filtering alone is not sufficient because models can guess tool names.

This issue owns the permission set semantics. Sub-agent work should reuse these rules rather than redefining a separate filtering model.
