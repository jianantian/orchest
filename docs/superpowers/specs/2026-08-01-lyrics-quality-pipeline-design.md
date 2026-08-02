# 歌词质量链路改造设计(music-gift demo)

日期:2026-08-01
状态:已评审(用户批准方向 C + 泛化约束)

## 背景

用户反馈:生成的歌词"太实"——复述对话素材、直呼名字、宣告情绪,接近"应酬诗",与人类精品歌词差距大。真实样本(gift `8b03b586a965`《小脚丫》,素材=孩子谷粒吃脚、父母想留住这一刻)四条全中:

1. 名字当钩子:"谷粒呀谷粒"在 chorus 唱 6 遍
2. 素材即剧情:verse 逐动作复述"坐地板→够脚→放嘴里"
3. 情感靠宣告:"你不知道自己多可爱""最柔软的画面"全是 tell
4. 零意象系统:唯一比喻是 cliché("像发现了宝藏"),题目"小脚丫"未被转化为 conceit

根因在上游方法论,不在 Suno 工程维度:

- `system.md` 只教"收集素材",不教"转化素材"
- `skills/lyrics-writer/SKILL.md` 全是工艺(结构/押韵/发音/cue),没有素材转化方法论
- `prompts/review.md` 明确禁止创作("NEVER add new content, new metaphors"),即使看出太实也不许改

## 目标 / 非目标

**目标**:链路级提升歌词艺术性——素材被转化为意象系统,而不是被复述。

**非目标**:

- 不改 Suno 工程维度(发音表、cue、段长限制等已对齐 bitwize-music)
- 不做歌词模型解耦(elevate/review 沿用 chat_model)
- 不建自动化歌词质量评测(验证靠同素材人工对比)
- 不动 countdown / music prompt 链路

## 泛化约束(用户明确要求)

链路必须可泛化,不加特殊逻辑:

1. **Rust 侧只新增一个通用"二次文本 pass"机制**;elevate 和现有 review 都是它的实例。不引入 per-song / per-language / per-name 特判。
2. **所有质量规则活在 prompt/skill 数据文件里**(`SKILL.md`、`system.md`、`elevate.md`);代码不感知"歌词""名字""中文"等概念。
3. elevate.md 里的规则是通用创作原则(种子/锚点/conceit),不绑定生日、孩子等具体场景。
4. 前端用单一"阶段指示"状态呈现 elevate/review,未来新增 pass 零前端改动。

## 设计

链路:`chat 生成 → elevate(创造性改稿) → review(机械纠错) → parse → 输出`

elevate 在 review **之前**:review 修发音/cue,必须作用于最终文本。

### 1. 素材转化方法论(改 prompt/skill 数据)

**`skills/lyrics-writer/SKILL.md`** 新增 `FROM MATERIAL TO ART` 章,置于 STRUCTURE 之前(创作第一原则):

- **The Seed Rule** — 素材是种子不是剧情
- **One Conceit** — 一首歌只立一个核心意象,段落从它生长、靠它递进
- **Anchor Rule(≤2)** — 真实细节最多保留 2 处,降格为意象锚点(藏,不点名);人名默认 0 次,最多 1 次且永不进 chorus
- **The Stranger Test** — 陌生人听到的必须是完整的歌;当事人"认出"而非"被告知"
- **留白** — 禁止情绪宣告,情绪由意象承载
- **Before/After 教材** — 《小脚丫》真实案例(应酬诗版 vs 转化版 + 逐条批注)+ 英文同构案例一组
- 13 点质检加第 14 点:**复述检查**(on-the-nose check)

**`prompts/system.md`** GENERATING 段加转化指令:写之前先定 conceit 和情感核,素材按 Seed Rule 处理(指向 skill 新章)。

### 2. Elevate pass(链路新阶段)

**`prompts/elevate.md`**(新文件,编译期 `include_str!`,同 review.md):

- 角色:改稿编辑(song doctor),不是校对
- 输入:初稿(带标签)。**不给对话历史**——避免重新锚定剧情;初稿中的名字/场景已足够识别素材
- 硬约束:保留情感核;锚点细节 ≤2 必须存活(防过度抽象);标签格式原样;可唱性不降(段长上限、chorus≥2、cue 保留);语言不变(中文进中文出)
- 幂等:初稿已达标(已立 conceit、无复述、锚点合规)时**原样返回**,不为改而改(follow-up 改稿会重走本 pass)
- 输出只有标签块,不附任何总结或说明(避免污染下游 review 输入)

**`agent.rs` — 通用二次文本 pass(泛化核心)**:

把现有 `run_review_pass` 泛化为:

```rust
pub struct PassOutcome { pub text: String, pub degraded: bool }

pub async fn run_text_pass(
    model: Arc<dyn ChatModel>,
    agent_name: &str,        // "music-gift/elevate" | "music-gift/review"
    stage: &'static str,     // 日志/degraded 标记用的阶段名
    system_prompt: &str,
    input: &str,
    validate: Option<&dyn Fn(&str) -> bool>,  // 输出门禁,见下
) -> PassOutcome
```

行为与现 review pass 一致(max_steps=1、无工具、失败/空输出回落输入文本 + `tracing::warn!(stage)` + degraded=true),另加:**`validate` 返回 false 时同样按 degraded 回落输入文本**。elevate 调用点传 `|s| parse_lyrics(s).has_lyrics`(创造性改写可能丢 `<<<LYRICS>>>` 标签,不拦截会让用户歌词整个丢失);review 传 `None` 保持现有行为——也给现有测试 `review_pass_sends_review_md_as_system_prompt`(mock 返回无标签文本)留了活路。机制通用、策略在调用点,符合泛化约束。

`run_review_pass` 保留为薄封装(固定 stage="review"、system_prompt=REVIEW_PROMPT),现有测试不动;`ELEVATE_PROMPT: include_str!("../prompts/elevate.md")` 与 REVIEW_PROMPT 并列。

**编排抽出为可测函数**:现 `chat_handler` 里 `tokio::spawn` 闭包内的后处理(门控 → 事件 → 两遍 pass → degraded 收集)提取为 `agent.rs` 的 `finalize_chat_output(model, full_text, tx) -> (String, Vec<String>)`,routes.rs 只调用它。可用 CaptureModel 注入做单测,且编排逻辑有了单一归属。

**`routes.rs` chat_handler**:调用 `finalize_chat_output`,随后 parse + 组 `Done` 事件;`degraded` 数组按实际失败阶段含 `"elevate"` / `"review"`。

**SSE 协议**:`SseEvent` 加 `Elevating` 单元变体(serde tag="type",additive,不破现有 wire 契约;api.ts 是 `as SseEvent` 强转无运行时校验,旧前端收到新事件安全忽略)。

### 3. 前端 / 测试 / 验证

**前端**(只有 `GuidedFlow.tsx` 消费 chat SSE,FreeCreate 不受影响):

- `types.ts`:`SseEvent` 加 `{ type: 'Elevating' }`
- `GuidedFlow.tsx`:把 `reviewing` 布尔态泛化为单一阶段态(`stage: 'elevate' | 'review' | null`),复用现有 typing-dots 指示器,label 取 `t("elevating")` / `t("reviewing")`
- degraded 提示泛化:现 `GuidedFlow.tsx:251` 只认 `degraded.includes("review")`,改为按 `degraded.length > 0` 驱动;`ReviewCard` 提示文案换成新的通用 i18n key(五语言,语义"部分歌词质量环节被跳过,已使用原始草稿"),替换掉 review 专属的 `review_skipped`
- `i18n.tsx`:五语言(zh/en/fr/es/ru)加 `elevating` key + 通用 degraded key

**测试**(CaptureModel 注入 `finalize_chat_output`):

- `run_text_pass`:system prompt / user turn 正确;失败或空输出回落输入 + degraded
- elevate 输出格式破坏(返回无 `<<<LYRICS>>>` 文本)→ validator 拦截,回落原稿 + degraded 含 "elevate"
- elevate 失败 → review 仍作用于原稿;双失败 → degraded = ["elevate", "review"]
- 现有 74+2 测试不动(review 不加 validator,`review_pass_sends_review_md_as_system_prompt` 语义不变)

**验证**:重启服务,用谷粒同素材(孩子/日常/想留住这一刻)重新生成,新旧歌词人工对比四条"实"是否消除。

**文档**:`docs/quality-gaps.md` 第 3 项(lyric-refiner future)更新为已落地的 transform pass;`docs/guided-pipeline.md` 阶段描述同步。

## 验收标准

- [ ] 同素材重新生成:名字不再进 chorus(≤1 次)、无逐动作复述、无情绪宣告句、存在贯穿 conceit
- [ ] elevate 失败时链路回落原稿,前端 degraded 提示可见(任一阶段降级均可见,不限 review)
- [ ] elevate 输出丢 `<<<LYRICS>>>` 标签时 validator 拦截、回落原稿(单测覆盖)
- [ ] `cargo test -p music-gift-demo` 全绿,clippy 零警告
- [ ] 新增 Rust 代码中无任何歌词/语言/人名特判(泛化约束走查;标签知识仅存在于既有 parse 层与调用点 validator)

## 风险与权衡

- **延迟**:chat 流程 +1 次完整 LLM 调用(几十秒);SSE 阶段事件使等待可感知,可接受
- **过度抽象**:elevate 可能把锚点洗没 → prompt 硬性规定锚点 ≤2 必须存活;失败兜底是原稿不会更差
- **格式破坏**:创造性改写可能丢标签 → 调用点 validator 拦截并回落原稿,兜底不会比现状差
- **教材语言**:SKILL.md 正文维持英文(与现有一致),案例中英各一
