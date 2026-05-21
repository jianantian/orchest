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

- [ ] frontmatter 包含以下字段（通过 `SkillScanner::scan` 解析后断言）：
  - `name: code-review`
  - `description`：长度 ≤ 200 字符
  - `allowed_tools`：明确声明本 skill 期望可用的 tool（包含 `read_file`，可选 `web_fetch`）
  - `bundled_tools`：声明 `summarize_diff` bundled tool，input schema 至少含 `diff: string`
  - `dependencies`：若需 Python 包则列出，否则字段为空 / 缺省（首选无依赖）
  - `capabilities`：声明 `network`、`filesystem.read`、`filesystem.write`、`env`、`max_memory_mb` 五个子项（值可为空数组 / false / null，但字段须存在）
- [ ] markdown body 满足以下可测约束：
  - 总字符数 ≥ 500
  - 包含至少 3 个 markdown 列表项形式的步骤指引
  - 包含至少 1 个明确的 "不做" / "不适用于" 段落（用 grep 检测形如 "本 skill 不"、"不要用本 skill"、"## 不适用" 的字符串）
  - 包含至少 1 处对 `summarize_diff` bundled tool 的调用说明
  - 包含至少 1 处对 `web_fetch`（来自 `orchest-tools`）的使用条件说明（"当需要查官方规范时调用 web_fetch"等）

### bundled script

- [ ] `scripts/summarize_diff.py` 从 stdin 读取 JSON `{"diff": "<unified-diff-text>"}`，向 stdout 输出 JSON `{"files_changed": N, "insertions": N, "deletions": N, "files": [{"path": "...", "insertions": N, "deletions": N}]}`
- [ ] 脚本仅使用 Python 标准库；如必须引入 dep，写入 SKILL.md 的 `dependencies.python` 并对应更新 capabilities
- [ ] 脚本本身的单元测试 `skills/code-review/tests/test_summarize_diff.py` 覆盖：空 diff、单文件 diff、多文件 diff、纯新增、纯删除、混合修改 6 个用例
- [ ] CI workflow 中追加 step：`python -m unittest discover skills/code-review/tests/`，exit code 0 视为通过

### 与 playground 集成

- [ ] 本 skill 是 issue 003 `v0_3_subagent_and_codeexec` scenario **唯一**加载的 skill；issue 003 的 v0.3 step 直接消费本 skill，断言条目见 issue 003：
  - `SkillScanner::scan(workspace_root.join("skills"))` 返回的 manifest 含 `name == "code-review"`
  - `CapabilityValidator::execution_env` 仅传入声明的环境变量
  - `summarize_diff` bundled tool 被调用并返回结构化结果
- [ ] 本 issue 在 `skills/code-review/` 下提供 `tests/sample_diff.txt` 作为 issue 003 scenario 复用的固定 input

### 与 SDK 文档集成

- [ ] `docs/sdk/authoring-skills.md` 引用本 skill 作为示例：贴 SKILL.md frontmatter、解释每个字段、链回本 skill 路径
- [ ] `skills/code-review/README.md` 解释 skill 用途、如何在自己项目中复用、如何修改

## 注意

- 这是给模型读的 SKILL.md，不是给人读的产品文档。procedural 内容要写得对模型友好：分点、明确触发条件、明确输出格式
- 不要在 skill 里调用 LLM API 自己——skill 是被 agent loop 消费的，agent 已经在与 LLM 通信
- `description` 字段是 progressive disclosure 的核心：模型在 tool listing 阶段只看这一行决定要不要展开本 skill 全文。写得太宽会被滥用，写得太窄不会被发现。建议参考 Anthropic Skills 官方示例的写法
- 不要在 skill 里硬编码具体语言或框架（如 "Python only" 或 "React only"），保持通用；具体场景由调用 skill 时的上下文决定
- 本 issue **不**要求实现第二个 skill；先把这一个跑稳，后续 skill 在 v0.5+ 追加
