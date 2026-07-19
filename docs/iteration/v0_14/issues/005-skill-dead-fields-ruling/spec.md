# 005 — 死字段/死变体裁定

## 背景

SDK-D5:`SkillManifest.allowed_tools` 除测试外无消费方;`capabilities` 只 enforce env(network/filesystem_read/filesystem_write/max_memory_mb 解析后从不生效);`RuntimeEvent::SkillDependencyError` 从不发射(死变体)。AGENTS.md 记载 v0.3 的锁定设计是"完成 ScriptExecutor trait 抽象和 capabilities 声明"——但"声明"与"enforce"的边界从未写清。

## 目标/范围

对每个死字段/死变体做明确裁定(不实现 sandbox):

- `allowed_tools`: 兑现(skill 加载后限制可用工具集)或显式标注"声明预留,暂不 enforce"——写进 rustdoc 与 polaris/design-principles.md。
- `capabilities` 的 network/filesystem/max_memory: 同上裁定(env 已 enforce,保持)。
- `SkillDependencyError`: 接上 env 构建失败路径(发射)或移除变体。

裁定原则(建议):v1.0 前不引入半成品 enforce;明确语义边界比模糊的死字段好。以 spec 文档 + rustdoc 为交付物,代码改动应小。

## 验收标准

- [ ] 每个字段/变体:要么有行为+测试,要么 rustdoc + design-principles.md 显式标注"声明预留,不 enforce"(语义边界写清)
- [ ] `SkillDependencyError`:发射(env 构建失败路径+测试)或移除(含 serde 兼容说明)
- [ ] `docs/polaris/design-principles.md` 的 capabilities 表述与裁定一致
- [ ] 五项检查全绿

## 备注

- 这是 v1.0 冻结前的语义清偿:宁可显式预留,不留"看起来 enforce 实际没有"的陷阱。
