# 009 · Skill 目录扫描与 Manifest 加载

## 背景

Runtime 启动时扫描指定 `skills_dir` 下的所有 `SKILL.md` 文件，解析 frontmatter 得到 `SkillManifest`，并把 `bundled_tools` 全部注册到 tool registry。

## 目标

实现 `SkillScanner`：扫描目录、解析 SKILL.md、返回 `SkillManifest` 列表。

## 验收标准

- [ ] `SkillScanner::scan(dir: &Path) -> Result<Vec<SkillManifest>, ScanError>` 实现
- [ ] 递归查找 `dir/**/{SKILL.md,skill.md}`（大小写不敏感匹配）
- [ ] SKILL.md 的 YAML frontmatter（`---` 包裹）解析为 `SkillManifest`：`name`、`description`、`allowed_tools`、`bundled_tools`
- [ ] `bundled_tools` 中每个 entry 包含：`name`、`description`、`executable`、`script`（相对路径）、`input_schema`
- [ ] `raw_frontmatter` 保留完整解析结果（`serde_json::Value`）
- [ ] 解析失败的 SKILL.md 记录警告并跳过，不影响其他 skill 加载
- [ ] `SkillManifest.path` 为 skill 目录的绝对路径
- [ ] `scan()` 只返回 `Vec<SkillManifest>`，**不**直接操作 `ToolRegistry`（职责分离，由调用方负责注册）
- [ ] skill 列表（name + description + path）可供 run loop 构造 system prompt

## 调用方职责

`SkillScanner::scan()` 的调用方（Agent 初始化逻辑）负责把每个 manifest 的 `bundled_tools` 注册进 `ToolRegistry`：

```rust
let manifests = SkillScanner::scan(&skills_dir)?;
for manifest in &manifests {
    for tool in manifest.bundled_tools() {
        registry.register(Arc::new(tool))?;
    }
}
```

## SKILL.md Frontmatter 格式示例

```yaml
---
name: research_topic
description: 用户需要调研某个主题、需要多源信息整合时使用。
allowed_tools:
  - web_search
  - read_file
bundled_tools:
  - name: fetch_arxiv
    description: 搜索 arXiv 论文
    executable: python
    script: scripts/fetch_arxiv.py
    input_schema:
      type: object
      properties:
        query: { type: string }
      required: [query]
---
```
