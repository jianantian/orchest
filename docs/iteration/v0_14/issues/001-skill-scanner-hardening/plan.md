# 001 — 实施计划

## 要读的文件

- `crates/orchest/src/skill/scanner.rs`(scan/extract_frontmatter/RawFrontmatter 全貌与现有单测 :154-295)
- `crates/orchest/src/run/skills.rs`(register_skills 如何消费扫描结果)
- `crates/orchest/src/events.rs`(事件变体命名/字段惯例)
- `docs/polaris/observability.md`(事件 vs tracing 的分工)

## 要改的文件

- `crates/orchest/src/skill/scanner.rs`
- `crates/orchest/src/events.rs`(若新增 SkillLoadWarning)
- `crates/orchest/src/run/skills.rs` / `run/actor.rs`(警告事件的 emit 路径)
- 测试

## 步骤

1. frontmatter 行级解析重写(保持 serde_yaml 反序列化部分不变);补边界单测。
2. 扫描失败的警告通道:scanner 返回结构化错误列表(或回调),`register_skills` 统一 emit 警告事件 + tracing::warn!。
3. 若新增事件变体:serde 兼容(skip/default)、Py/Node 透传检查。
4. 测试:坏 YAML、不可读文件、description 含 `---`、CRLF、空 frontmatter、无 frontmatter。
5. 五项检查。
