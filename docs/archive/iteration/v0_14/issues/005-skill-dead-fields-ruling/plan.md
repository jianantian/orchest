# 005 — 实施计划

## 要读的文件

- `crates/orchest/src/skill/types.rs`(allowed_tools/capabilities 字段)
- `crates/orchest/src/run/skills.rs`(注册时对这些字段的现有处理/警告)
- `crates/orchest/src/skill/env_manager.rs`(capabilities env enforce 现状)
- `crates/orchest/src/events.rs:139`(SkillDependencyError 死变体)
- `docs/polaris/design-principles.md:47` 附近(capabilities 表述)
- AGENTS.md 的锁定设计条目(v0.3 ScriptExecutor/capabilities)

## 要改的文件

- 裁定涉及的类型 rustdoc
- `crates/orchest/src/events.rs`(SkillDependencyError 接上或移除)
- `docs/polaris/design-principles.md`(语义边界)
- 测试(若有行为)

## 步骤

1. 逐字段裁定(与维护者确认倾向:显式预留 vs 兑现)。
2. allowed_tools/capabilities:rustdoc 写明"声明预留,不 enforce"(或兑现);design-principles.md 同步。
3. SkillDependencyError:接 env 构建失败路径(+测试)或移除(serde 兼容说明)。
4. 五项检查。
