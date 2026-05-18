# 011 · Builtin read_file Tool

## 背景

Agent 通过 `read_file` tool 按需读取 skill 的 SKILL.md 和 references 文件，这是 skill 渐进式披露机制的核心操作。该 tool 是 runtime 内置，不需要用户注册。

## 目标

实现 `ReadFileTool`，自动注册到 `ToolRegistry`，并在被调用时发出 `SkillContentRead` 事件。

## 验收标准

- [ ] `ReadFileTool` 实现 `Tool` trait，`source` 为 `ToolSource::Builtin`
- [ ] input schema：`{ "path": { "type": "string", "description": "文件路径" } }`
- [ ] `execute()` 读取文件内容，返回文件文本作为 `ToolOutput::Immediate(Value::String(content))`
- [ ] 文件不存在或读取失败时返回 `ToolError`（不 panic）
- [ ] 当读取的路径匹配已注册 skill 的 SKILL.md 路径时，发出 `SkillContentRead { skill_name, file, tokens }` 事件
- [ ] `tokens` 为粗略估算（字符数 / 4），不调用 tokenizer
- [ ] v0.1 不限制路径（path allowlist 是 v0.2 安全增强）
- [ ] **仅当**读取路径匹配已注册 skill 的 SKILL.md 时发出 `SkillContentRead`；读取其他任意文件不产生专用审计事件（这是已知限制，不是完整审计）

## 说明

`SkillContentRead` 只覆盖 skill 内容读取，不是全量文件访问审计。v0.1 的安全依赖"用户信任 skill 来源"；v0.2 通过 path allowlist 收紧，届时可以对所有读取发出审计事件。
