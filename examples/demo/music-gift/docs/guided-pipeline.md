# 引导模式完整链路

## 总览

```
用户打开页面 → 关系 → 名字 → 性别 → 生日 → 场景 → 聊天(AI写词) → 审核 → 生成音乐 → 完成
                  └─ 宠物则跳过性别/生日 ─┘
```

每一步前端渲染交互组件，用户选择后状态写入 `useGuidedState` hook（持久化到 `sessionStorage`），推进到下一步。

---

## Step 1: 关系 (relationship)

**用户看到：** 6 个 pills — 孩子 / 伴侣 / 朋友 / 父母 / 宠物 / 其他… + 纯音乐快捷入口

**实现：**

- `GuidedFlow.tsx:286-289` — `PillsRow` 渲染关系选项，`GoldPill` 渲染纯音乐入口
- `GuidedFlow.tsx:16-20` — `RELATIONSHIPS` 常量定义，label 走 `t()` i18n
- 用户选择 → `handleRelPick(value, label)` → 写入 `meta.relationship` / `meta.relationshipLabel`，`act.go("name")` 或 `act.go("scenario")`（宠物跳过名字/性别/生日直达场景）

**数据落点：** `meta: { relationship: "kid", relationshipLabel: "孩子" }`

---

## Step 2: 名字 (name)

**用户看到：** 文本输入框 + OK 按钮

**实现：**

- `GuidedFlow.tsx:291` — `InlineInput` 组件，placeholder 走 `t("name_placeholder")`
- 用户提交 → `handleName(name)` → 写入 `meta.name`，`act.go("gender")` 或 `act.go("scenario")`（宠物）

**数据落点：** `meta.name = "小明"`

---

## Step 3: 性别 (gender)

**用户看到：** 2 个 pills — 男生 / 女生

**实现：**

- `GuidedFlow.tsx:321` — `PillsRow`，选项从 `t("gender_male")` / `t("gender_female")` 取
- 用户选择 → `handleGender(label)` → 写入 `meta.gender`，`act.go("birthday")`

**数据落点：** `meta.gender = "女生"`

---

## Step 4: 生日 (birthday)

**用户看到：** 月份网格（12 个月按钮）+ 选中月份后出现日期输入 + 跳过按钮

**实现：**

- `GuidedFlow.tsx:323` — `BirthdayPicker` 组件，接收 `months`（从 `getMonths(lang)` 取，5 种语言各自翻译）、`skipLabel`、`dayPlaceholder`
- `ChatUI.tsx:88-146` — `BirthdayPicker` 实现：月份网格 + 日期 number input，日期有范围校验
- 用户选择 → `handleBirthday(bday)` → 写入 `meta.birthday = { month: 4, day: 20 }`，`act.go("scenario")`
- 跳过 → `onPick(null)` → `meta.birthday = null`

**数据落点：** `meta.birthday = { month: 4, day: 20 }` 或 `null`

---

## Step 5: 场景 (scenario)

**用户看到：** 根据关系展示 3-4 个场景 pills + "自己说…"

**实现：**

- `GuidedFlow.tsx:325` — `PillsRow`，选项来自 `scenarioList(meta.relationship, t)`
- `GuidedFlow.tsx:22-61` — `scenarioList()` 根据关系返回场景 key → 通过 `t(key)` 取 i18n label，value 为稳定的英文关键词（如 `"birthday"`, `"daily_life"`）
- 场景 label 全部 i18n，5 种语言各自翻译（`i18n.tsx` 中 `scen_*` 系列 key）
- 用户选择 → `handleScenario(value, label)` → 写入 `meta.scenario` / `meta.scenarioLabel`，`act.go("chat")`，触发 `startChat(null)`

**数据落点：** `meta.scenario = "birthday"`, `meta.scenarioLabel = "生日"`

---

## Step 6: 聊天 — AI 写词 (chat)

**用户看到：** 对话气泡流式输出，AI 追问 1-2 个问题收集素材，然后生成歌词

**前端：**

- `GuidedFlow.tsx:144-228` — `startChat()`：
  1. 把 `meta` 打包为 `{ lang, name, relationship, scenario, gender, birthday }` 发给 `/api/chat`
  2. SSE 流式接收：`Delta`（逐字展示）、`Reviewing`（进入审核阶段，显示等待提示）、`Done`（解析歌词/风格/歌名/人声到 `draft`）
  3. 有歌词 → `act.go("review")`

- `api.ts:21-65` — `streamChat()`：`fetch` + `ReadableStream` 逐行解析 SSE

**后端：**

- `routes.rs:89-155` — `chat_handler`：
  1. `build_system_message(meta)` 组装 system prompt = `system.md` + `lyrics.md` + "Known info:\n{meta JSON}"
  2. 启动 `run_chat_agent()` → Orchest AgentRun，max 5 steps，注册 `collect_info` tool
  3. 流式输出 Delta 事件
  4. Agent 跑完后如果有 `<<<LYRICS>>>`，启动 **review pass**（第二个 LLM 调用）
  5. 先发 `Reviewing` 事件告知前端等待
  6. `run_review_pass()` 用 `review.md` 做 10 点审核：自动修复发音/performance cues/artist names，标记结构/押韵/双胞胎 verse 等问题
  7. 审核完成后解析 `ParsedLyrics`，发 `Done` 事件

- `agent.rs:178-232` — `run_chat_agent()`：Orchest AgentRun，max 5 steps
- `agent.rs:245-312` — `run_review_pass()`：独立的单步 AgentRun，用 `review.md` 做系统 prompt，输入为原始输出，无 tool

**LLM 调用的 prompt：**

| 阶段 | Prompt 文件 | 作用 |
|------|------------|------|
| System | `prompts/system.md` | 对话风格、追问策略、何时生成 |
| Lyrics | `prompts/lyrics.md` | 写词方法论：结构、押韵方案、音节、Show Don't Tell、13 点质量检查、发音修正、performance cues |
| Review | `prompts/review.md` | 10 点审核清单：自动修复发音、performance cues、artist names；标记结构/押韵等问题 |

**数据落点：** `draft = { lyrics: "...", style: "温柔轻快", title: "挥手的魔法", vocal: "female" }`

---

## Step 7: 审核歌词 (review)

**用户看到：** `ReviewCard` 组件 — 可编辑的歌词、风格、人声、歌名，底部"生成这首歌"按钮

**实现：**

- `GuidedFlow.tsx:336` — `ReviewCard` 渲染，接收 `draft` 数据 + `DEFAULT_STYLE_TAGS`（17 个风格标签供选择）
- 用户可编辑任意字段
- 点击生成 → `handleReviewSubmit(data)`：
  1. `gen.start({ lyrics, style, title, vocal, meta, lang })` — 创建 gift + 触发音乐生成
  2. `act.go("music")`

---

## Step 8: 生成音乐 (music)

**用户看到：** `MusicCard` 组件 — 进度指示 → 完成后显示试听入口

**前端：**

- `useMusicGen.ts:54-98` — `start()`：
  1. `POST /api/gift` — 创建 gift 记录（含 lyrics/style/title/vocal/meta）
  2. `POST /api/generate/:id` — 触发音乐生成
  3. `EventSource(/api/generate/:id/stream)` — SSE 监听生成状态

- `useMusicGen.ts:20-52` — `watchStream()`：SSE 监听，`ready` → 设置 state 为 ready

**后端（`routes.rs:434-481` — `generate_music`）：**

1. 从 gift store 读取 gift 数据
2. **歌词校验** — `validate_lyrics()` 检查 word count / section limits 等，只打 warning 不阻断
3. **Music prompt 改写** — `generate_music_prompt()` 调用 LLM：

   | 输入 | 来源 |
   |------|------|
   | lyrics | gift.lyrics（Chat LLM 生成的原始歌词） |
   | style | gift.meta.style（Chat LLM 的 `<<<STYLE>>>` 输出） |
   | title | gift.meta.title |
   | vocal | gift.meta.vocal |
   | scene | gift.meta.scenario（场景关键词） |
   | name | gift.meta.name |
   | relationship | gift.meta.relationship |
   | lang | gift.meta.lang |

   LLM 用 `prompts/music_prompt/suno.md` 模板，输出结构化 `EnrichedPrompt`：
   ```
   prompt:        "female, breathy, legato, indie folk, warm nostalgia, acoustic guitar, cello, soft piano"
   genre:         ["indie folk", "chamber pop"]
   tempo:         "ballad-slow"
   mood:          ["warm", "nostalgic", "bittersweet"]
   vocal_style:   "female, breathy, legato"
   instrumentation: "acoustic guitar, cello, soft piano, brushed drums"
   production:    "spacious reverb, lo-fi warmth"
   exclude:       "no backing vocals, no heavy drums"
   style_tags:    ["indie folk", "acoustic", "ballad"]
   ```

4. **Style prompt 校验** — `check_style_prompt()` 检查是否有 artist names 泄漏
5. `MusicGenTool.submit()` — 提交到 Suno/Mureka API，返回后后台轮询

**LLM 调用汇总：**

| 调用 | 触发时机 | Prompt | 模型 | 耗时 |
|------|---------|--------|------|------|
| Chat agent | 用户进入 chat 阶段 | `system.md` + `lyrics.md` | `chat_model` | ~10-30s |
| Review pass | Chat agent 生成完歌词后 | `review.md` | `chat_model`（复用） | ~10-30s |
| Music prompt | 用户点"生成这首歌"后 | `music_prompt/suno.md` | `music_prompt_model` | ~3-5s |

---

## 状态管理

所有引导流程状态由 `useGuidedState` hook 管理，持久化到 `sessionStorage`（key: `moment_guided`）：

```ts
{
  lang: "zh",
  step: "chat",           // 当前步骤
  meta: { ...StepMeta },  // 关系/名字/性别/生日/场景
  messages: [...],         // 聊天记录
  draft: { lyrics, style, title, vocal } | null
}
```

刷新页面可恢复。`clearGuided()` 清除状态重新开始。

---

## 涉及文件

| 层 | 文件 | 职责 |
|----|------|------|
| 前端 | `frontend/src/components/GuidedFlow.tsx` | 引导流程主组件，所有步骤渲染 + 状态流转 |
| 前端 | `frontend/src/components/ChatUI.tsx` | `PillsRow` / `InlineInput` / `BirthdayPicker` / `GoldPill` |
| 前端 | `frontend/src/components/ReviewCard.tsx` | 歌词审核卡片，可编辑 |
| 前端 | `frontend/src/components/MusicCard.tsx` | 生成进度 + 完成状态 |
| 前端 | `frontend/src/hooks/useGuidedState.ts` | 引导状态管理，sessionStorage 持久化 |
| 前端 | `frontend/src/hooks/useMusicGen.ts` | 音乐生成生命周期：创建 gift → 触发生成 → SSE 监听 |
| 前端 | `frontend/src/api.ts` | API 客户端：`streamChat` / `createGift` / `generateMusic` |
| 前端 | `frontend/src/i18n.tsx` | 5 语言字典 + `useI18n` context |
| 后端 | `src/routes.rs` | `/api/chat` + `/api/gift` + `/api/generate/:id` + `/api/polish-music-prompt` |
| 后端 | `src/agent.rs` | `run_chat_agent` + `run_review_pass` + `parse_lyrics` |
| 后端 | `src/agent/message.rs` | `build_system_message` + `build_messages` |
| 后端 | `src/prompts.rs` | 编译期嵌入 `system.md` / `lyrics.md` / `countdown.md`，启动期加载 `music_prompt/*.md` |
| 后端 | `src/tools/music_gen.rs` | `generate_music_prompt` + `MusicGenTool`（提交/轮询/流） |
| Prompt | `prompts/system.md` | Chat agent 系统 prompt |
| Prompt | `prompts/lyrics.md` | 写词方法论 skill |
| Prompt | `prompts/review.md` | 歌词审核 skill |
| Prompt | `prompts/music_prompt/suno.md` | Music prompt 改写 skill |
