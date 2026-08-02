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

- `GuidedFlow.tsx` — `RELATIONSHIPS` 常量定义选项，`PillsRow` 渲染关系 pills，`GoldPill` 渲染纯音乐入口，label 走 `t()` i18n
- 用户选择 → `handleRelPick(value, label)` → 写入 `meta.relationship` / `meta.relationshipLabel`，`act.go("name")` 或 `act.go("scenario")`（宠物跳过名字/性别/生日直达场景）

**数据落点：** `meta: { relationship: "kid", relationshipLabel: "孩子" }`

---

## Step 2: 名字 (name)

**用户看到：** 文本输入框 + OK 按钮

**实现：**

- `GuidedFlow.tsx` — `InlineInput` 组件，placeholder 走 `t("name_placeholder")`
- 用户提交 → `handleName(name)` → 写入 `meta.name`，`act.go("gender")` 或 `act.go("scenario")`（宠物）

**数据落点：** `meta.name = "小明"`

---

## Step 3: 性别 (gender)

**用户看到：** 2 个 pills — 男生 / 女生

**实现：**

- `GuidedFlow.tsx` — `PillsRow`，选项从 `t("gender_male")` / `t("gender_female")` 取
- 用户选择 → `handleGender(label)` → 写入 `meta.gender`，`act.go("birthday")`

**数据落点：** `meta.gender = "女生"`

---

## Step 4: 生日 (birthday)

**用户看到：** 月份网格（12 个月按钮）+ 选中月份后出现日期输入 + 跳过按钮

**实现：**

- `GuidedFlow.tsx` — `BirthdayPicker` 组件，接收 `months`（从 `getMonths(lang)` 取，5 种语言各自翻译）、`skipLabel`、`dayPlaceholder`
- `ChatUI.tsx` — `BirthdayPicker` 实现：月份网格 + 日期 number input，日期有范围校验（`daysInMonth`）
- 用户选择 → `handleBirthday(bday)` → 写入 `meta.birthday = { month: 4, day: 20 }`，`act.go("scenario")`
- 跳过 → `onPick(null)` → `meta.birthday = null`

**数据落点：** `meta.birthday = { month: 4, day: 20 }` 或 `null`

---

## Step 5: 场景 (scenario)

**用户看到：** 根据关系展示 3-4 个场景 pills + "自己说…"

**实现：**

- `GuidedFlow.tsx` — `PillsRow`，选项来自 `scenarioList(meta.relationship, t)`
- `scenarioList()` 根据关系返回场景 key → 通过 `t(key)` 取 i18n label，value 为稳定的英文关键词（如 `"birthday"`, `"daily_life"`）
- 场景 label 全部 i18n，5 种语言各自翻译（`i18n.tsx` 中 `scen_*` 系列 key）
- 用户选择 → `handleScenario(value, label)` → 写入 `meta.scenario` / `meta.scenarioLabel`，`act.go("chat")`，触发 `startChat(null)`

**数据落点：** `meta.scenario = "birthday"`, `meta.scenarioLabel = "生日"`

---

## Step 6: 聊天 — AI 写词 (chat)

**用户看到：** 对话气泡流式输出，AI 追问 1-2 个问题收集素材，然后生成歌词

**前端：**

- `GuidedFlow.tsx` — `startChat()`：
  1. 把 `meta` 打包为 `{ lang, name, relationship, scenario, gender, birthday }` 发给 `/api/chat`（relationship / scenario 发的是显示 label；birthday 为 `"M-D"` 字符串，跳过则不传）
  2. SSE 流式接收：`Delta`（逐字展示）、`Elevating`（进入改稿阶段，显示等待提示）、`Reviewing`（进入审核阶段，显示等待提示）、`Done`（解析歌词/风格/歌名/人声到 `draft`，审核报告到 `review`）
  3. 有歌词 → `act.go("review")`

- `api.ts` — `streamChat()`：`fetch` + `ReadableStream` 逐行解析 SSE

**后端：**

- `routes.rs` — `chat_handler`：
  1. `build_system_message(meta)` 组装 system prompt = `system.md` + "Known info:\n{meta JSON}"（有照片时追加照片提示）
  2. 启动 `run_chat_agent()` → Orchest AgentRun，max 5 steps，注册 `collect_info` tool；`skills_dir` 由 SDK 自动注入 `<available_skills>` 清单并注册内置 `load_skill` / `read_file`（v0.14 零配置披露）
  3. 流式输出 Delta 事件
  4. Agent 跑完后如果输出含 `<<<LYRICS>>>`,由 `finalize_chat_output()` 依次跑两个二轮 pass(各一次完整 LLM 调用):
     1. 先发 `Elevating` 事件 → `run_text_pass(stage="elevate", prompts/elevate.md)` 创造性改稿:素材→意象转化(种子规则/单一 conceit/锚点≤2/陌生人测试/禁宣告),防"应酬诗";输出无 `<<<LYRICS>>>` 时调用点 validator 拦截并回落原稿
     2. 再发 `Reviewing` 事件 → `run_review_pass()` 用 `review.md` 做 10 点审核:自动修复发音/performance cues/artist names,标记结构/押韵/双胞胎 verse 等问题(作用于升华后的最终文本)
  5. 解析 `ParsedLyrics`,并用 `extract_review_summary()` 提取审核报告,随 `Done` 事件一并发给前端;`degraded` 数组标记回落阶段(`"elevate"` / `"review"`)

- `agent.rs` — `run_chat_agent()`:Orchest AgentRun,max 5 steps
- `agent.rs` — `run_text_pass()`(通用单步二轮 AgentRun,elevate/review 共用) + `run_review_pass()` + `finalize_chat_output()`(编排:门控 → Elevating → elevate → Reviewing → review);`elevate.md` / `review.md` 编译期 `include_str!` 嵌入,无 tool

**写词方法论的加载（渐进式披露）：**

写词方法论不在 system prompt 里，而是一个独立的 Agent Skill：`skills/lyrics-writer/SKILL.md`。设 `skills_dir` 后 SDK 自动把 skill 清单注入 system prompt（零配置渐进式披露）；`system.md` 指示 agent 写词前先加载 `lyrics-writer` skill，模型通过内置 `load_skill` 工具按需加载正文，然后严格按方法论输出。

**LLM 调用的 prompt：**

| 阶段 | Prompt 文件 | 作用 |
|------|------------|------|
| System | `prompts/system.md` | 对话风格、追问策略、何时生成；指示 agent 写词前先加载 lyrics-writer skill（SDK 披露清单指引 `load_skill` 调用） |
| Lyrics | `skills/lyrics-writer/SKILL.md` | 写词方法论(agent 按需加载):素材转化(FROM MATERIAL TO ART)、结构、押韵方案、音节、Show Don't Tell、14 点质量检查、发音修正、performance cues |
| Review | `prompts/review.md` | 10 点审核清单：自动修复发音、performance cues、artist names；标记结构/押韵等问题 |
| Elevate | `prompts/elevate.md` | 创造性改稿:种子规则、单一 conceit、锚点≤2、陌生人测试、禁宣告;幂等(达标原样返回);输出仅标签块 |

**数据落点：** `draft = { lyrics: "...", style: "温柔轻快", title: "挥手的魔法", vocal: "female" }`，`review = "## Review Pass ..."`（审核报告 Markdown，可空）；`Done.degraded` 标记回落阶段（`"elevate"` / `"review"`，空数组=全部正常），任一阶段降级时审核卡片显示通用提示

---

## Step 7: 审核歌词 (review)

**用户看到：** `ReviewCard` 组件 — 可编辑的歌词、风格、人声、歌名，底部"生成这首歌"按钮；有审核报告时可展开查看（带 🔧 修复计数徽标）

**实现：**

- `GuidedFlow.tsx` — `ReviewCard` 渲染，接收 `draft` 数据 + `getStyleTags(lang)`（i18n 风格标签，每语言 8 个）+ `review` 审核报告
- 用户可编辑任意字段
- 点击生成 → `handleReviewSubmit(data)`：
  1. `gen.start({ lyrics, style, title, vocal, meta, lang })` — 创建 gift + 触发音乐生成
  2. `act.go("music")`

---

## Step 8: 生成音乐 (music)

**用户看到：** `MusicCard` 组件 — 进度指示 → 完成后显示试听入口

**前端：**

- `useMusicGen.ts` — `start()`：
  1. `POST /api/gift` — 创建 gift 记录（含 lyrics/style/title/vocal/meta）
  2. `POST /api/generate/:id` — 触发音乐生成
  3. `EventSource(/api/generate/:id/stream)` — SSE 监听生成状态

- `useMusicGen.ts` — `watchStream()`：SSE 监听，`done` → state 置为 `ready`，`failed` → `error`

**后端（`routes.rs` — `generate_music`）：**

0. **Provider 选择** — `MUSIC_GIFT_MUSIC_PROVIDER` 环境变量（默认 `suno`），决定 prompt 模板和生成后端
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

   LLM 用 `prompts/music_prompt/{provider}.md` 模板（suno / mureka / minimax，编译期 `include_str!` 嵌入，不依赖运行时工作目录），输出结构化 `EnrichedPrompt`：
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
5. `music_gen::generate()` → `submit()` — 提交到由 orchest-provider registry 构建的对应 provider `GenTask`，返回后后台轮询

另外：`POST /api/gift` 创建 gift 时若 meta 带生日，后台会并行触发 countdown 页面生成子流程（`tools/countdown.rs`），失败会把 `countdown_status` 置为 `failed`。

**LLM 调用汇总：**

| 调用 | 触发时机 | Prompt | 模型 | 耗时 |
|------|---------|--------|------|------|
| Chat agent | 用户进入 chat 阶段 | `system.md`（写词时按需经 `load_skill` 加载 `skills/lyrics-writer/SKILL.md`） | `chat_model` | ~10-30s |
| Review pass | Chat agent 生成完歌词后 | `review.md` | `chat_model`（复用） | ~10-30s |
| Elevate pass | Chat agent 生成完歌词后(review 之前) | `elevate.md` | `chat_model`(复用) | ~10-30s |
| Music prompt | 用户点"生成这首歌"后 | `music_prompt/{provider}.md` | `music_prompt_model` | ~3-5s |

---

## 状态管理

所有引导流程状态由 `useGuidedState` hook 管理，持久化到 `sessionStorage`（key: `moment_guided`）：

```ts
{
  lang: "zh",
  step: "chat",           // 当前步骤
  meta: { ...StepMeta },  // 关系/名字/性别/生日/场景
  messages: [...],         // 聊天记录（只保留最近 20 条）
  draft: { lyrics, style, title, vocal } | null
}
```

刷新页面可恢复。`clearGuided()` 清除状态重新开始。

恢复规则:`review` 及之前的步骤都可恢复;`music` 不可恢复(giftId 不持久化,恢复后 MusicCard 无法渲染、chat bar 被禁用),加载时回退为 `review`,由已持久化的 draft 重新提交生成。

---

## 涉及文件

| 层 | 文件 | 职责 |
|----|------|------|
| 前端 | `frontend/src/components/GuidedFlow.tsx` | 引导流程主组件，所有步骤渲染 + 状态流转 |
| 前端 | `frontend/src/components/ChatUI.tsx` | `PillsRow` / `InlineInput` / `BirthdayPicker` / `GoldPill` |
| 前端 | `frontend/src/components/ReviewCard.tsx` | 歌词审核卡片，可编辑，可展开审核报告 |
| 前端 | `frontend/src/components/MusicCard.tsx` | 生成进度 + 完成状态 |
| 前端 | `frontend/src/hooks/useGuidedState.ts` | 引导状态管理，sessionStorage 持久化 |
| 前端 | `frontend/src/hooks/useMusicGen.ts` | 音乐生成生命周期：创建 gift → 触发生成 → SSE 监听 |
| 前端 | `frontend/src/api.ts` | API 客户端：`streamChat` / `createGift` / `generateMusic` |
| 前端 | `frontend/src/i18n.tsx` | 5 语言字典 + `useI18n` context + `getMonths` / `getStyleTags` |
| 后端 | `src/routes.rs` | `/api/chat` + `/api/gift` + `/api/generate/:id` + `/api/polish-music-prompt` |
| 后端 | `src/agent.rs` | `run_chat_agent` + `run_text_pass`(通用二轮 pass,elevate/review 共用) + `run_review_pass` + `finalize_chat_output` + `parse_lyrics` + `extract_review_summary`;编译期嵌入 `review.md` / `elevate.md` |
| 后端 | `src/agent/message.rs` | `build_system_message` + `build_messages` |
| 后端 | `src/prompts.rs` | 编译期 `include_str!` 嵌入 `system.md` / `countdown.md` / `music_prompt/*.md`（suno/mureka/minimax） |
| 后端 | `src/tools/music_gen.rs` | `generate_music_prompt` + `generate` / `submit` / `poll` / `stream`（模块函数） |
| Prompt | `prompts/system.md` | Chat agent 系统 prompt |
| Skill | `skills/lyrics-writer/SKILL.md` | 写词方法论，agent 通过内置 `load_skill` 按需加载 |
| Prompt | `prompts/review.md` | 歌词审核清单（编译期嵌入 `agent.rs`） |
| Prompt | `prompts/elevate.md` | 创造性改稿 prompt(编译期嵌入 `agent.rs`) |
| Prompt | `prompts/music_prompt/{suno,mureka,minimax}.md` | Music prompt 改写模板，按 provider 选择 |
