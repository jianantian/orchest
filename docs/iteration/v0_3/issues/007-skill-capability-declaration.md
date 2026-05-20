# 007 · SKILL.md capability 声明与 CapabilityValidator

## 背景

Skill bundled script 需要网络访问、特定环境变量、文件系统权限，但 v0.2 的 `SkillManifest` 对此没有任何声明机制。没有这份数据，未来的沙箱策略就无从实施。

本 issue 在 SKILL.md frontmatter 中引入 `capabilities` 字段，并在 runtime 侧实现 `CapabilityValidator`，为后续沙箱强制执行打基础。

## 目标

定义 `capabilities` 字段 schema，实现 `CapabilityValidator`，使 `ExecutionContext.env` 只包含声明的变量。

## 验收标准

**SKILL.md schema：**
- [ ] `SkillManifest` 新增 `capabilities: Option<SkillCapabilities>` 字段
- [ ] `SkillCapabilities` 结构体：`network: bool`、`filesystem_read/write: Vec<PathBuf>`、`env: Vec<String>`、`max_memory_mb: Option<u32>`
- [ ] SKILL.md 解析器正确处理 `capabilities` 缺失的情况（`None`，不报错）

**CapabilityValidator：**
- [ ] Skill 含有 `scripts/` 目录但 `capabilities` 为 `None` 时，发出 `SkillMissingCapabilities { skill_name }` 警告事件（不阻断执行）
- [ ] `ExecutionContext.env` 由 `capabilities.env` 列表从父进程环境中选取；列表为空或 `capabilities` 为 `None` 时，`env` 为空 map

**文档：**
- [ ] 示例 skill 补充 `capabilities` 字段声明
- [ ] SKILL.md 格式说明中增加 `capabilities` 字段描述

## 说明

v0.3 的 `CapabilityValidator` 只做警告，不做硬性阻断。`filesystem_read/write` 和 `max_memory_mb` 在 v0.3 中记录但不强制——这些字段的存在是为了让未来的沙箱实现能直接读取，不需要再改 schema。
