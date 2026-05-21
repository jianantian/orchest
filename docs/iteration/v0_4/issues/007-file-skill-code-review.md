# 007 · 文件型 skill 参考实现：code-review

## 背景

仓内 `skills/` 目前是空目录。文件型 skill 是这次三类"非 core 能力"中最贴近 Anthropic Skills 标准的形态：纯文件、procedural 知识、跨语言天然可用。这次借 v0.4 给出第一个完整参考实现，作为后续 skill 的样板。

选 `code-review` 是因为：
- 是 SDK 用户的高频场景
- 自然组合多种基础能力：读文件（builtin `read_file`）、可选的 web 获取（`orchest-tools` 的 `WebFetchTool`）、可选的 sub-agent
- 不依赖外部专有 API

## 目标

在 `skills/code-review/` 落一个完整可被 runtime 加载、被 playground v0.3 scenario 验收、被 `docs/sdk/authoring-skills.md` 当作示例引用的 skill。

## 验收标准

### 目录与文件

```
skills/code-review/
├── SKILL.md
├── scripts/
│   └── summarize_diff.py        # bundled tool 示例脚本
├── prompts/
│   └── review-checklist.md      # 可选：被 SKILL.md 引用的 procedural 内容
└── README.md                    # 给人读的说明（不是给模型读的）
```

### SKILL.md 内容

- [ ] frontmatter 包含以下字段：
  - `name: code-review`
  - `description`：≤ 200 字，让模型能判断何时触发本 skill
  - `allowed_tools`：明确声明本 skill 期望可用的 tool（如 `read_file`、`web_fetch`），不依赖未声明的 tool
  - `bundled_tools`：声明 `summarize_diff` bundled tool，给出 input schema
  - `dependencies`：若 bundled 脚本依赖 Python 包，列出依赖（保持最少；首选无依赖）
  - `capabilities`：完整声明 `network`、`filesystem.read`、`filesystem.write`、`env`、`max_memory_mb`
- [ ] markdown body 包含 procedural 指引：检查清单、何时调用 `web_fetch` 查规范、何时启动 sub-agent 处理大 diff、输出格式约定
- [ ] 至少有一个"反例"小节：说明本 skill **不**做什么（避免被滥用）

### bundled script

- [ ] `scripts/summarize_diff.py` 读取标准输入中的 JSON（包含 diff 文本），输出 JSON 摘要（变更文件数、新增/删除行数、按文件分组的变更预览）
- [ ] 脚本无 Python 依赖（仅标准库）；如必须引入 dep，写入 SKILL.md 的 `dependencies.python`
- [ ] 脚本不读环境变量、不访问网络、不写文件系统——保证 capability 校验通过即可运行

### 与 playground 集成

- [ ] Issue 003 的 `v0_3_subagent_and_codeexec` scenario 中加载 `skills/code-review/`，验证：
  - `SkillScanner::scan(workspace_root.join("skills"))` 发现该 skill
  - `CapabilityValidator` 成功构造 `ExecutionContext.env`
  - mock model 调用 `summarize_diff` bundled tool，事件流中出现 `ToolCallStarted { source: Skill { skill_name: "code-review" } }`

### 与 SDK 文档集成

- [ ] `docs/sdk/authoring-skills.md` 引用本 skill 作为示例：贴 SKILL.md frontmatter、解释每个字段、链回本 skill 路径
- [ ] `skills/code-review/README.md` 解释 skill 用途、如何在自己项目中复用、如何修改

## 注意

- 这是给模型读的 SKILL.md，不是给人读的产品文档。procedural 内容要写得对模型友好：分点、明确触发条件、明确输出格式
- 不要在 skill 里调用 LLM API 自己——skill 是被 agent loop 消费的，agent 已经在与 LLM 通信
- `description` 字段是 progressive disclosure 的核心：模型在 tool listing 阶段只看这一行决定要不要展开本 skill 全文。写得太宽会被滥用，写得太窄不会被发现。建议参考 Anthropic Skills 官方示例的写法
- 不要在 skill 里硬编码具体语言或框架（如 "Python only" 或 "React only"），保持通用；具体场景由调用 skill 时的上下文决定
- 本 issue **不**要求实现第二个 skill；先把这一个跑稳，后续 skill 在 v0.5+ 追加
