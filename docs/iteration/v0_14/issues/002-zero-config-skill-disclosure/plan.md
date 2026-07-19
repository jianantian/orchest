# 002 — 实施计划

## 要读的文件

- `crates/orchest/src/run/skills.rs`(register_skills 现状)
- `crates/orchest/src/skill/scanner.rs` + `skill/types.rs`(SkillManifest 形状)
- `crates/orchest/src/run/actor.rs`(pre_start 消息组装、skills_dir 流向)
- `crates/orchest/src/tool/builtin.rs`(ReadFileTool 与 SkillContentRead 遥测,:140-166)
- `crates/orchest/src/skill/bundled_tool.rs:101-108`(canonicalize + 前缀检查先例)
- `crates/orchest/src/run/config.rs`(AgentConfig/builder,加披露开关的位置)
- Anthropic Agent Skills 官方规范(docs/external 或网络,确认三级披露的推荐格式)

## 要改的文件

- `crates/orchest/src/run/skills.rs`(元数据注入 + load_skill 注册)
- `crates/orchest/src/skill/`(load_skill 工具实现,新文件或并入现有模块)
- `crates/orchest/src/run/config.rs`(披露开关)
- `crates/orchest/src/run/actor.rs`(system prompt 注入点)
- `crates/orchest-py` / `crates/orchest-node`(开关透传)
- 测试

## 步骤

1. 设计注入块格式(如 `<available_skills><skill><name>…</name><description>…</description></skill>…</available_skills>` + 一行"需要时调用 load_skill"的指引);确定注入点(system prompt 尾部或无 system 时自成一条)。
2. `skill_disclosure` 配置(默认开);builder/绑定透传。
3. `load_skill` 工具:name → 正文 + bundled 清单;可选 path → 单文件内容;canonicalize + 前缀检查;`SkillContentRead` 遥测。
4. 注册进 ToolRegistry(与 bundled_tools 同路径,检查重名)。
5. 测试:注入块、开关、正文/清单/资源、逃逸、未知名、遥测。
6. 五项检查。
