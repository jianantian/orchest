# 001 — skill scanner 错误上报 + frontmatter 加固

## 背景

`crates/orchest/src/skill/scanner.rs`:
- `:103-107` 对读取/YAML 错误一律 `.ok()?` 返回 None,skill 无声消失;`scan_recursive`(`:74`)对 `read_dir` 失败同样静默——"skill 没加载"类问题零线索。
- frontmatter 用 `find("---")` 切分(`:149`),description 含 `---` 即提前截断(配合静默吞错 = skill 消失)。

## 目标/范围

1. **错误上报**: 解析失败产生结构化警告——首选 `RuntimeEvent`(若无合适变体,新增如 `SkillLoadWarning { path, reason }`;遵循 observability.md 的命名/字段惯例)+ `tracing::warn!` 兜底。警告必须带文件路径与原因(YAML 错、IO 错、frontmatter 缺失等)。
2. **frontmatter 加固**: 改为行级解析——只认独立一行的 `---` 作为结束符(开头 `---` 后必须换行);description 内含 `---` 不再截断。

## 验收标准

- [ ] 坏 frontmatter / 坏 YAML / 不可读文件:产生含路径与原因的结构化警告,其余 skill 正常加载
- [ ] description 含 `---` 的合法 skill 解析正确(不再截断)
- [ ] 开头无 `---` 的文件按无 frontmatter 处理(警告)
- [ ] 事件变体(若新增)进 Py/Node wire 透传(serde 兼容)
- [ ] 测试:警告内容、frontmatter 边界(含 `---` 的 description、CRLF、空 frontmatter)
- [ ] 五项检查全绿

## 备注

- 本 issue 是 002(披露)的前置:披露需要可靠的 manifest + 可见的失败。
