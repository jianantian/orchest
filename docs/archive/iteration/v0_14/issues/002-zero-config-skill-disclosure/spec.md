# 002 — 零配置渐进式披露(元数据注入 + load_skill)

## 背景

运行时从不把 skill 的 name/description 注入 system prompt,也没有任何 skill 列表通道(`crates/orchest/src/run/skills.rs:17-97`);纯知识 skill 完全靠应用手写路径 + 模型自觉 `read_file`(music-gift system.md:65-68)。与 polaris「渐进式披露作为一等抽象」(overview.md:17)存在落差;2026-07-18 确认设计目标为**非常易用**——设 `skills_dir` 后披露链路全自动,脆弱用法不应再是必要手段。

## 目标/范围

对齐 Anthropic Agent Skills 三级披露,零配置默认开启:

1. **Level 1 元数据常驻**: 扫描 `skills_dir` 后,将全部 skill 的 name+description 以固定格式块(如 `<available_skills>`)注入 system prompt 尾部(无 system prompt 时自成一条);默认开启,`skill_disclosure: Off`(或等价配置)可关。
2. **Level 2 正文按需加载**: 内置 `load_skill` 工具(name 参数),返回该 skill 的 SKILL.md 正文 + bundled 文件清单;路径由 runtime 按扫描结果解析(**消除 CWD 依赖**);命中即发 `SkillContentRead` 遥测。
3. **Level 3 资源按需**: `load_skill` 支持可选 path 参数加载 SKILL.md 引用的 bundled 文件;canonicalize + 前缀检查防逃逸(复用 `bundled_tool.rs:101-108` 的做法)。

bundled_tools 现有注册流程不变;披露与其互补。

## 验收标准

- [x] **易用性硬指标**: 全新使用方只放 SKILL.md 目录 + 设 `skills_dir`——模型首次调用即看到可用 skill 列表(name+description),调用 `load_skill` 得正文;全程无需手写路径、无需注册工具、无 CWD 依赖
- [x] 注入格式稳定(便于快照/测试断言);`skill_disclosure` 关闭时行为与现状一致(无任何注入)
- [x] `load_skill` 的路径逃逸防护(相对路径 `..`、绝对路径)有测试;未知名称返回结构化错误
- [x] `SkillContentRead` 遥测覆盖 load_skill 路径(原 read_file 路径不回归)
- [x] 测试:注入块内容、load_skill 正文/清单/资源加载、逃逸、未知名、关闭开关
- [x] 五项检查全绿

## 备注

- demo 的 lyrics-writer 是本 issue 的实证用例(optimization-plan D7 正式方案),demo 侧改造不在本迭代。
- Py/Node 绑定:披露开关需透传(若绑定暴露了 skills_dir,则同路径透传开关)。
