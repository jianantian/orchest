# 歌词质量链路(elevate pass + 素材转化方法论)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 music-gift demo 的歌词链路中加入创造性 elevate pass 并把"素材转化"方法论写进 skill,解决歌词"太实"(应酬诗)问题。

**Architecture:** 链路变为 `chat 生成 → elevate(创造性改稿) → review(机械纠错) → parse`。Rust 侧把现有 `run_review_pass` 泛化为通用 `run_text_pass`(机制通用、策略在调用点),所有质量规则只活在 prompt/skill 数据文件里。设计规格:`docs/superpowers/specs/2026-08-01-lyrics-quality-pipeline-design.md`。

**Tech Stack:** Rust(axum/tokio/orchest SDK)、React 19 + TS + Vite、SQLite。demo 根目录:`examples/demo/music-gift/`(下文相对路径均相对 repo 根)。

## Global Constraints

- **泛化约束**:新增 Rust 代码不得引入歌词/语言/人名/场景特判;质量规则只写进 prompt/skill 数据文件。标签知识只允许出现在既有 parse 层与调用点 validator。
- **CI 必须绿**:`cargo test -p music-gift-demo`、`cargo clippy -p music-gift-demo --all-targets -- -D warnings`、`cargo fmt --check`。
- **commit 确认**:每个 Task 末尾的 commit 步骤,执行前必须先获得用户确认(会话规则,高于计划)。commit message 遵循 repo 约定:前缀 `feat:`/`refactor:`/`docs:`,subject <72 字符。
- 前端无单测框架,验证 = `cd examples/demo/music-gift/frontend && npm run build`(含 `tsc -b` 类型检查)。
- 现有 74+2 测试不得被破坏;`run_review_pass` 的现有测试 `review_pass_sends_review_md_as_system_prompt` 语义不变。

---

### Task 1: 通用二次文本 pass `run_text_pass`

**Files:**
- Modify: `examples/demo/music-gift/src/agent.rs:314-402`(REVIEW_PROMPT / ReviewOutcome / run_review_pass 区块)

**Interfaces:**
- Consumes: 现有 `AgentConfig` / `AgentRun` / `RuntimeEvent` / `RunInput`(orchest)、`tracing`
- Produces:
  ```rust
  pub struct PassOutcome { pub text: String, pub degraded: bool }
  pub async fn run_text_pass(
      model: Arc<dyn ChatModel>,
      agent_name: &str,
      stage: &'static str,
      system_prompt: &str,
      input: &str,
      validate: Option<&dyn Fn(&str) -> bool>,
  ) -> PassOutcome
  pub async fn run_review_pass(model: Arc<dyn ChatModel>, raw_output: &str) -> PassOutcome // 薄封装
  ```

- [ ] **Step 1: 写失败测试**

在 `agent.rs` 的 `#[cfg(test)] mod tests` 中追加(放在现有 `review_pass_sends_review_md_as_system_prompt` 测试之后):

```rust
    #[tokio::test]
    async fn text_pass_validator_rejection_falls_back_to_input() {
        let model = Arc::new(CaptureModel::default());
        let outcome = run_text_pass(
            model,
            "music-gift/test",
            "test",
            "SYS",
            "RAW INPUT",
            Some(&|s: &str| s.contains("<<<LYRICS>>>")),
        )
        .await;
        assert!(outcome.degraded);
        assert_eq!(outcome.text, "RAW INPUT");
    }

    #[tokio::test]
    async fn text_pass_validator_acceptance_passes_output_through() {
        let model = Arc::new(CaptureModel::default());
        let outcome = run_text_pass(model, "music-gift/test", "test", "SYS", "RAW INPUT", Some(&|_s: &str| true))
            .await;
        assert!(!outcome.degraded);
        assert_eq!(outcome.text, "done");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-gift-demo text_pass 2>&1 | tail -5`
Expected: 编译错误 `cannot find function 'run_text_pass' in this scope`

- [ ] **Step 3: 实现**

把 `agent.rs:314-402` 的 `REVIEW_PROMPT` static、`ReviewOutcome` struct、`run_review_pass` 函数整体替换为:

```rust
/// Review system prompt compiled into the binary.
static REVIEW_PROMPT: &str = include_str!("../prompts/review.md");

/// Outcome of a second-pass text stage (elevate / review): the (possibly
/// untouched) text plus a degradation flag the caller surfaces to the client.
pub struct PassOutcome {
    pub text: String,
    /// True when the pass did not run to completion (or its output was
    /// rejected by the call-site validator) and `text` is the input unchanged.
    pub degraded: bool,
}

/// Run a single-step second-pass agent over `input` and return its text.
///
/// Generic mechanism shared by every post-generation stage (elevate, review):
/// the stage's identity (`agent_name`, `stage` for logs/degraded markers) and
/// its prompt come from the caller, so no stage-specific logic lives here.
/// On any failure — config error, run failure, empty output, or `validate`
/// rejecting the output — the input text is returned unchanged with
/// `degraded: true`; a fallen-back stage must never be worse than its input.
pub async fn run_text_pass(
    model: Arc<dyn ChatModel>,
    agent_name: &str,
    stage: &'static str,
    system_prompt: &str,
    input: &str,
    validate: Option<&dyn Fn(&str) -> bool>,
) -> PassOutcome {
    let config = match AgentConfig::builder(agent_name)
        .max_steps(1)
        .system_prompt(system_prompt)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(stage, error = %e, "text pass: building config failed; keeping input");
            return PassOutcome {
                text: input.to_string(),
                degraded: true,
            };
        }
    };

    // The stage prompt rides as the system prompt and the input text is the
    // single user turn — no flattening, so the agent keeps its role.
    let run_input = RunInput::text(input);
    let tool_registry = ToolRegistry::new(); // Second-pass stages use no tools.
    let (handle, mut rx) = AgentRun::start(config, run_input, model, tool_registry);

    let mut out = String::new();
    let mut run_failed = false;
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ModelStreamChunk {
                delta: StreamEvent::Text { delta: text },
            } => {
                out.push_str(&text);
            }
            RuntimeEvent::RunCompleted { output, .. } => {
                if out.is_empty() {
                    if let Some(text) = output.as_str() {
                        out = text.to_string();
                    }
                }
            }
            RuntimeEvent::RunFailed { error, .. } => {
                tracing::warn!(stage, error = %error, "text pass: agent run failed; keeping input");
                run_failed = true;
            }
            _ => {}
        }
    }
    handle.wait().await;

    let rejected = out.is_empty() || validate.is_some_and(|v| !v(&out));
    if rejected {
        if out.is_empty() {
            if !run_failed {
                tracing::warn!(stage, "text pass: empty output; keeping input");
            }
        } else {
            tracing::warn!(stage, "text pass: output rejected by validator; keeping input");
        }
        PassOutcome {
            text: input.to_string(),
            degraded: true,
        }
    } else {
        tracing::debug!(stage, chars = out.len(), "text pass: done");
        PassOutcome {
            text: out,
            degraded: false,
        }
    }
}

/// Run a second-pass review agent on the raw chat output.
///
/// The reviewer checks pronunciation, performance cues, structure,
/// and content issues using a 10-point checklist derived from
/// bitwize-music's lyric-reviewer skill (CC0).
///
/// Returns corrected output in the same tag format. Falls back to
/// the original on error — every fallback is logged and flagged degraded.
pub async fn run_review_pass(model: Arc<dyn ChatModel>, raw_output: &str) -> PassOutcome {
    run_text_pass(model, "music-gift/review", "review", REVIEW_PROMPT, raw_output, None).await
}
```

同时把现有测试里 `review_pass_sends_review_md_as_system_prompt` 的返回类型使用保持不变(`outcome.degraded` / `outcome.text` 字段名不变,无需改测试)。

- [ ] **Step 4: 跑全部测试**

Run: `cargo test -p music-gift-demo 2>&1 | tail -8`
Expected: 76 passed(74 原有 + 2 新增);`ReviewOutcome` 无残留:
Run: `rg "ReviewOutcome" examples/demo/music-gift/src || echo "clean"`
Expected: `clean`

- [ ] **Step 5: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/src/agent.rs
git commit -m "refactor: generalize review pass into run_text_pass with output validator"
```

---

### Task 2: 抽取 `finalize_chat_output` 编排函数

**Files:**
- Modify: `examples/demo/music-gift/src/agent.rs`(`run_review_pass` 之后新增函数 + tests)
- Modify: `examples/demo/music-gift/src/routes.rs:110-167`(chat_handler 的 spawn 闭包)

**Interfaces:**
- Consumes: Task 1 的 `run_review_pass` / `PassOutcome`;现有 `SseEvent`、`mpsc`
- Produces:
  ```rust
  pub async fn finalize_chat_output(
      model: Arc<dyn ChatModel>,
      full_text: &str,
      tx: &mpsc::Sender<SseEvent>,
  ) -> (String, Vec<String>)  // (最终文本, 回落阶段名列表)
  ```

- [ ] **Step 1: 写失败测试**

追加到 `agent.rs` tests mod:

```rust
    #[tokio::test]
    async fn finalize_skips_passes_for_conversational_output() {
        let model = Arc::new(CaptureModel::default());
        let (tx, mut rx) = mpsc::channel::<SseEvent>(16);
        let (text, degraded) = finalize_chat_output(model.clone(), "just chatting", &tx).await;
        assert_eq!(text, "just chatting");
        assert!(degraded.is_empty());
        assert!(model
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty());
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn finalize_runs_review_when_lyrics_present() {
        let model = Arc::new(CaptureModel::default());
        let (tx, mut rx) = mpsc::channel::<SseEvent>(16);
        let (text, degraded) =
            finalize_chat_output(model.clone(), "<<<LYRICS>>>\nla la\n<<<END>>>", &tx).await;
        assert_eq!(text, "done");
        assert!(degraded.is_empty());
        assert_eq!(
            model
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            1
        );
        assert!(matches!(rx.recv().await, Some(SseEvent::Reviewing)));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p music-gift-demo finalize 2>&1 | tail -5`
Expected: 编译错误 `cannot find function 'finalize_chat_output' in this scope`

- [ ] **Step 3: 实现**

在 `agent.rs` 的 `run_review_pass` 之后追加:

```rust
/// Post-process a completed chat turn: when the raw output carries a lyric
/// block, run the second-pass stages over it and collect the stages that
/// fell back. Stage boundaries are announced on `tx` so the client can label
/// the wait. Pure orchestration — parsing happens at the call site.
pub async fn finalize_chat_output(
    model: Arc<dyn ChatModel>,
    full_text: &str,
    tx: &mpsc::Sender<SseEvent>,
) -> (String, Vec<String>) {
    if !full_text.contains("<<<LYRICS>>>") {
        return (full_text.to_string(), Vec::new());
    }
    let _ = tx.send(SseEvent::Reviewing).await;
    let reviewed = run_review_pass(model, full_text).await;
    let degraded = if reviewed.degraded {
        vec!["review".to_string()]
    } else {
        Vec::new()
    };
    (reviewed.text, degraded)
}
```

替换 `routes.rs` chat_handler 中 `match result` 的 `Ok(full_text)` 分支(现 routes.rs:122-157)为:

```rust
            Ok(full_text) => {
                let had_lyrics = full_text.contains("<<<LYRICS>>>");
                let (final_text, degraded) =
                    crate::agent::finalize_chat_output(review_model, &full_text, &tx).await;
                let parsed = parse_lyrics(&final_text);
                // The review summary only exists when a review pass ran.
                let review = if had_lyrics {
                    crate::agent::extract_review_summary(&final_text)
                } else {
                    None
                };
                let done = SseEvent::Done {
                    has_lyrics: parsed.has_lyrics,
                    lyrics: parsed.lyrics,
                    style: parsed.style,
                    title: parsed.title,
                    vocal: parsed.vocal,
                    review,
                    degraded,
                };
                let _ = tx.send(done).await;
            }
```

注意:原分支里的 `has_lyrics` / `reviewed` / `review_degraded` 局部变量全部删除,`review_model` 变量(routes.rs:112)保留继续传给 `finalize_chat_output`。

- [ ] **Step 4: 跑全部测试 + clippy**

Run: `cargo test -p music-gift-demo 2>&1 | tail -5 && cargo clippy -p music-gift-demo --all-targets -- -D warnings 2>&1 | tail -3`
Expected: 78 passed;clippy 无输出(零警告)

- [ ] **Step 5: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/src/agent.rs examples/demo/music-gift/src/routes.rs
git commit -m "refactor: extract finalize_chat_output orchestration from chat_handler"
```

---

### Task 3: Elevate pass(elevate.md + SseEvent::Elevating + 编排接入)

**Files:**
- Create: `examples/demo/music-gift/prompts/elevate.md`
- Modify: `examples/demo/music-gift/src/agent.rs`(SseEvent 枚举、ELEVATE_PROMPT、finalize_chat_output 重写、tests)

**Interfaces:**
- Consumes: Task 1 `run_text_pass`、Task 2 `finalize_chat_output`、既有 `parse_lyrics`
- Produces:
  - `SseEvent::Elevating` 单元变体(serde `tag="type"`,wire 为 `{"type":"Elevating"}`,additive 不破既有契约)
  - `static ELEVATE_PROMPT: &str`(编译期 `include_str!`)
  - `finalize_chat_output` 新行为:`Elevating → elevate(validator=has_lyrics) → Reviewing → review`,degraded 数组按序含 `"elevate"` / `"review"`

- [ ] **Step 1: 创建 `prompts/elevate.md`(完整内容)**

```markdown
You are a song doctor — an editor who rewrites lyric drafts. You do NOT
proofread. Your ONLY job is to take a draft that retells its source material
too literally and transform it into art.

═══ YOUR INPUT ═══

You will receive a raw draft containing lyrics, style, title, and vocal
annotations (the format below). The draft was written from a real person's
brief — it may name people, retell events beat by beat, and announce feelings
directly.

═══ WHAT "TOO LITERAL" LOOKS LIKE ═══

- Names sung as the hook (a name chanted in every chorus)
- Verses that replay the memory action by action, like a diary
- Feelings announced outright ("you are so lovely", "I will miss you",
  "this is my softest moment") instead of embodied in images
- No central image: details are listed, nothing organizes the song

═══ YOUR REWRITE ═══

Transform the draft along these rules:

1. ONE CONCEIT — Choose a single central image the whole song grows from
   (a pair of little feet that haven't touched mud yet; headlights receding
   down a street). Every section develops it or contrasts with it.
2. ANCHOR RULE (≤2) — Preserve at most two concrete details from the draft
   (a name, a place, an object, a gesture), and demote them to anchors:
   hidden inside images, never the subject of a chorus. A personal name
   appears at most once in the whole song, and NEVER in a chorus. Default
   to zero.
3. THE STRANGER TEST — A stranger must hear a complete song, not a greeting
   card. The person it's for should RECOGNIZE it, not be TOLD it.
4. SHOW, NEVER ANNOUNCE — Delete emotional declarations; let the imagery
   carry the feeling. If a line states the feeling ("so lovely", "I miss
   you"), rewrite it into an image.
5. PRESERVE THE EMOTIONAL CORE — The feeling underneath the draft (what the
   giver wants the recipient to feel) must survive intact. Transform the
   telling, keep the truth.

═══ HARD CONSTRAINTS ═══

- Output ONLY the tagged block below — no commentary, no change log, no
  explanations. Anything outside the tags corrupts downstream parsing.
- Keep the EXACT tag format: <<<LYRICS>>> … <<<END>>> then <<<STYLE>>>,
  <<<TITLE>>>, <<<VOCAL>>> blocks (same order as the input).
- Language is unchanged: Chinese in → Chinese out, English in → English out.
- Singability must not regress: keep the section-tag structure, keep every
  section within its length limits, keep ≥2 choruses, and keep a performance
  cue on every section tag (add one if a tag is bare).
- Keep pronunciation fixes (phonetic spellings like "liv"/"red") intact.
- TITLE: you may re-title to fit the new conceit (2-6 word visual image,
  never an abstract emotion word). STYLE/VOCAL pass through unchanged unless
  the draft's are empty.

<<<LYRICS>>>
... (rewritten lyrics — every section tag has a cue)
<<<END>>>
<<<STYLE>>>...<<<STYLE_END>>>
<<<TITLE>>>...<<<TITLE_END>>>
<<<VOCAL>>>...<<<VOCAL_END>>>

═══ IDEMPOTENCY ═══

If the draft ALREADY meets the bar — one conceit, no plot retelling, anchors
within budget, no announced feelings — return it UNCHANGED. Do not rewrite
for the sake of rewriting. Later editing rounds send drafts back through
you; they must not drift further from the source material each time.
```

- [ ] **Step 2: 写失败测试(ScriptedModel + 3 个用例)**

在 `agent.rs` tests mod 中,`CaptureModel` 定义之后追加脚本化 mock:

```rust
    /// A ChatModel that plays back a scripted sequence of replies (or
    /// failures), one per `complete` call, and captures the message lists.
    struct ScriptedModel {
        steps: Mutex<std::collections::VecDeque<Result<String, String>>>,
        calls: Mutex<Vec<Vec<Message>>>,
    }

    impl ScriptedModel {
        fn new(steps: Vec<Result<String, String>>) -> Self {
            Self {
                steps: Mutex::new(steps.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ChatModel for ScriptedModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "scripted"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(messages.to_vec());
            let step = self
                .steps
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_front()
                .unwrap_or(Ok("done".to_string()));
            match step {
                Ok(text) => Ok(ModelResponse {
                    content: vec![ContentBlock::Text(text)],
                    usage: TokenUsage::default(),
                    stop_reason: StopReason::EndTurn,
                    option_adjustments: vec![],
                }),
                Err(msg) => Err(ModelError::internal(msg, "mock_failure")),
            }
        }
    }
```

再追加三个测试:

```rust
    /// Elevate returns tag-less prose → the validator rejects it, the review
    /// pass still runs on the ORIGINAL draft, and "elevate" is marked degraded.
    #[tokio::test]
    async fn elevate_output_without_lyric_tags_falls_back_but_review_still_runs() {
        let model = Arc::new(ScriptedModel::new(vec![
            Ok("sorry, here is some prose without tags".to_string()),
            Ok("<<<LYRICS>>>\nreviewed\n<<<END>>>".to_string()),
        ]));
        let (tx, mut rx) = mpsc::channel::<SseEvent>(16);
        let draft = "<<<LYRICS>>>\noriginal\n<<<END>>>";
        let (text, degraded) = finalize_chat_output(model.clone(), draft, &tx).await;

        assert_eq!(text, "<<<LYRICS>>>\nreviewed\n<<<END>>>");
        assert_eq!(degraded, vec!["elevate".to_string()]);
        let calls = model
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(calls.len(), 2);
        // The review pass must receive the original draft, not the prose.
        let Some(ContentBlock::Text(review_user)) = calls[1][1].content.first() else {
            panic!("user turn must be text");
        };
        assert_eq!(review_user, draft);
        drop(calls);
        assert!(matches!(rx.recv().await, Some(SseEvent::Elevating)));
        assert!(matches!(rx.recv().await, Some(SseEvent::Reviewing)));
    }

    /// Both stages succeed: the review pass runs on the ELEVATED text, the
    /// stage events fire in order, nothing is degraded.
    #[tokio::test]
    async fn elevate_then_review_pipeline_order() {
        let model = Arc::new(ScriptedModel::new(vec![
            Ok("<<<LYRICS>>>\nelevated\n<<<END>>>".to_string()),
            Ok("<<<LYRICS>>>\nreviewed\n<<<END>>>".to_string()),
        ]));
        let (tx, mut rx) = mpsc::channel::<SseEvent>(16);
        let (text, degraded) =
            finalize_chat_output(model.clone(), "<<<LYRICS>>>\noriginal\n<<<END>>>", &tx).await;

        assert_eq!(text, "<<<LYRICS>>>\nreviewed\n<<<END>>>");
        assert!(degraded.is_empty());
        let calls = model
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(ContentBlock::Text(review_user)) = calls[1][1].content.first() else {
            panic!("user turn must be text");
        };
        assert_eq!(review_user, "<<<LYRICS>>>\nelevated\n<<<END>>>");
        drop(calls);
        assert!(matches!(rx.recv().await, Some(SseEvent::Elevating)));
        assert!(matches!(rx.recv().await, Some(SseEvent::Reviewing)));
    }

    /// Both stages fail: the original draft survives and both are reported.
    #[tokio::test]
    async fn elevate_and_review_failures_are_both_reported() {
        let model = Arc::new(ScriptedModel::new(vec![
            Err("elevate boom".to_string()),
            Err("review boom".to_string()),
        ]));
        let (tx, _rx) = mpsc::channel::<SseEvent>(16);
        let draft = "<<<LYRICS>>>\noriginal\n<<<END>>>";
        let (text, degraded) = finalize_chat_output(model, draft, &tx).await;
        assert_eq!(text, draft);
        assert_eq!(degraded, vec!["elevate".to_string(), "review".to_string()]);
    }
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p music-gift-demo elevate 2>&1 | tail -5`
Expected: 编译错误(找不到 `SseEvent::Elevating` / `ELEVATE_PROMPT`)

- [ ] **Step 4: 实现**

3a. `agent.rs` 的 `SseEvent` 枚举中,在 `Reviewing` 变体**之前**插入:

```rust
    /// Emitted when the creative elevation pass starts (after the chat stream
    /// ends, before `Reviewing`). Also a full LLM call — the client must not
    /// sit silent with a disabled input.
    Elevating,
```

3b. 在 `REVIEW_PROMPT` static 旁追加:

```rust
/// Creative elevation prompt compiled into the binary.
static ELEVATE_PROMPT: &str = include_str!("../prompts/elevate.md");
```

3c. 把 Task 2 的 `finalize_chat_output` 整体替换为:

```rust
/// Post-process a completed chat turn: when the raw output carries a lyric
/// block, run the second-pass stages over it and collect the stages that
/// fell back. Stage boundaries are announced on `tx` so the client can label
/// the wait. Pure orchestration — parsing happens at the call site.
///
/// Stage order matters: the creative elevation runs first, and the
/// mechanical review (pronunciation, cues) runs on the final text so its
/// fixes survive to the music provider.
pub async fn finalize_chat_output(
    model: Arc<dyn ChatModel>,
    full_text: &str,
    tx: &mpsc::Sender<SseEvent>,
) -> (String, Vec<String>) {
    if !full_text.contains("<<<LYRICS>>>") {
        return (full_text.to_string(), Vec::new());
    }
    let mut degraded: Vec<String> = Vec::new();

    let _ = tx.send(SseEvent::Elevating).await;
    let elevated = run_text_pass(
        model.clone(),
        "music-gift/elevate",
        "elevate",
        ELEVATE_PROMPT,
        full_text,
        // A creative rewrite can drop the lyric tags, which would discard
        // the user's song at parse time. Reject and keep the original draft.
        Some(&|s: &str| parse_lyrics(s).has_lyrics),
    )
    .await;
    if elevated.degraded {
        degraded.push("elevate".to_string());
    }

    let _ = tx.send(SseEvent::Reviewing).await;
    let reviewed = run_review_pass(model, &elevated.text).await;
    if reviewed.degraded {
        degraded.push("review".to_string());
    }

    (reviewed.text, degraded)
}
```

- [ ] **Step 5: 跑全部测试 + clippy**

Run: `cargo test -p music-gift-demo 2>&1 | tail -5 && cargo clippy -p music-gift-demo --all-targets -- -D warnings 2>&1 | tail -3`
Expected: 81 passed;clippy 零警告

- [ ] **Step 6: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/prompts/elevate.md examples/demo/music-gift/src/agent.rs
git commit -m "feat: add creative elevate pass before lyric review"
```

---

### Task 4: SKILL.md 加 FROM MATERIAL TO ART 方法论

**Files:**
- Modify: `examples/demo/music-gift/skills/lyrics-writer/SKILL.md`(frontmatter 之后的正文区)

**Interfaces:**
- Consumes: 无(纯数据文件,运行时经 `load_skill` 加载)
- Produces: `FROM MATERIAL TO ART` 章 + 14 点质检(原 13 点加第 14 点复述检查)

- [ ] **Step 1: 插入新章节**

在 `SKILL.md` 的 `Adapted from bitwize-music's lyric-writer skill (CC0). Key reference tables below.` 一行之后、`═══ STRUCTURE ═══` 之前,插入:

```markdown
═══ FROM MATERIAL TO ART (READ FIRST) ═══

The brief gives you raw material: a name, a relationship, a memory. Do not
transcribe it — transform it. A lyric that retells the brief is a greeting
card, not a song.

THE SEED RULE
The memory is the seed, not the plot. "She put her foot in her mouth and I
couldn't stop laughing" is material. The song is not about foot-in-mouth; it
is about what the moment means. Grow the song from the meaning; let the
memory surface as at most one image.

ONE CONCEIT
Choose ONE central image and let the whole song grow from it. Verses develop
it, the chorus distills it, the bridge turns it. If a line doesn't feed the
conceit, cut it. Scattered nice images are not a substitute — one image,
deepened, beats five images, listed.

THE ANCHOR RULE (≤2)
Keep at most two concrete details from the brief (name, place, object,
gesture) — and demote them to anchors: hidden inside imagery, never the
subject of a line, NEVER in a chorus. A personal name appears at most once
in the entire song, and its default count is zero. The song must belong to
the feeling, not to the proper nouns.

THE STRANGER TEST
Read the lyric as a stranger who knows nothing about the brief. They must
hear a complete song that stands on its own. The person it's for should
RECOGNIZE themselves in it — not be TOLD about themselves.

SHOW, NEVER ANNOUNCE
Delete emotional declarations. "You are so lovely" / "I want to keep this
moment" / "I miss you" are conclusions, not lyrics. Find the image that
makes the listener reach the conclusion themselves. (This extends the SHOW
DON'T TELL section below from the line level to the whole song.)

═══ WORKED EXAMPLE 1 (Chinese) ═══

Brief: 孩子谷粒, 日常 — the parent watched the baby put a foot in her mouth
and wanted to keep the moment forever.

BEFORE (transcription — everything wrong):
  [chorus]
  谷粒呀谷粒               ← name chanted as the hook (Anchor Rule violated)
  你不知道自己多可爱        ← feeling announced (Show, Never Announce violated)
  这一刻我想留住
  你吃脚的样子             ← the memory IS the plot (Seed Rule violated)
  是我心里最柔软的画面      ← conclusion stated, no image

AFTER (transformation — conceit: feet too small to have touched mud):
  [chorus]
  小脚丫 还没沾过泥         ← the conceit, established; the memory hidden
  先踩进了我心里           ←   inside it (one anchor, transformed)
  不急着长大 不急着走       ← the feeling embodied as a wish about time,
  这双手还抱得住你            never announced

What changed and why: the name is gone from the chorus (0 occurrences); the
foot-eating scene survives only as "还没沾过泥" — those who know, know;
"可爱 / 想留住" became "不急着长大" — the same feeling, reached through
the image.

═══ WORKED EXAMPLE 2 (English) ═══

Brief: for Dad's 60th — he taught me to ride a bike; I want to thank him.

BEFORE:
  [verse 1]
  You held the saddle tight
  And ran beside the bike
  I was scared but you were there
  On Maple Street that summer night    ← beat-by-beat replay of the memory

  [chorus]
  Thank you Dad, thank you Dad         ← role chanted as the hook
  For everything you do
  You're the best dad in the world     ← announced, generic, greeting-card

AFTER (conceit: the hand that let go):
  [verse 1]
  You were running still beside me
  Long after I could ride
  Both my hands were on the handlebar
  Your laughter trailing just behind   ← the memory compressed into one image
                                          that already contains the theme

  [chorus]
  Every road I've ever taken
  Started with a hand that let me go   ← gratitude embodied in the conceit;
  I never heard you running behind       "dad" appears nowhere
  You made it look like I rode alone
```

- [ ] **Step 2: 13 点质检升级为 14 点**

把 `═══ QUALITY CHECK (13-Point) ═══` 改为 `═══ QUALITY CHECK (14-Point) ═══`,并在第 13 条之后追加:

```markdown
14. ☐ Transcription: no beat-by-beat retelling of the brief, no name in
    chorus (≤1 name total), no announced feelings, one conceit holds the
    whole song together
```

同时在文件末尾 `REMEMBER:` 清单的 `- Run the 13-point quality check before presenting` 一行改为:

```markdown
- Run the 14-point quality check before presenting — including #14 (no
  transcription of the brief)
```

- [ ] **Step 3: 校验插入点**

Run: `grep -n "FROM MATERIAL TO ART\|14-Point\|WORKED EXAMPLE" examples/demo/music-gift/skills/lyrics-writer/SKILL.md`
Expected: 各命中 1+ 次;新章节位于 STRUCTURE 章之前(`grep -n "═══" SKILL.md | head -6` 确认顺序)

- [ ] **Step 4: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/skills/lyrics-writer/SKILL.md
git commit -m "feat: teach lyrics-writer material-to-art transformation"
```

---

### Task 5: system.md 生成指令加转化要求

**Files:**
- Modify: `examples/demo/music-gift/prompts/system.md`(GENERATING 段,现 63-72 行)

**Interfaces:**
- Consumes: Task 4 的 skill 新章节名(`FROM MATERIAL TO ART`)
- Produces: 无新签名(纯 prompt 数据)

- [ ] **Step 1: 改写 GENERATING 段**

把 `═══ GENERATING ═══` 段的 `Once loaded, generate lyrics following the methodology exactly. Output with` 一句替换为两句(其余行不动):

替换前:
```
Once loaded, generate lyrics following the methodology exactly. Output with
<<<LYRICS>>>, <<<STYLE>>>, <<<TITLE>>>, <<<VOCAL>>>, and <<<END>>> tags
exactly as specified. Don't announce what you're doing — just do it.
```

替换后:
```
Once loaded, generate lyrics following the methodology exactly. The memories
they gave you are the seed, not the plot: before writing, decide the ONE
central image (conceit) the song grows from and the feeling underneath the
story. Follow the FROM MATERIAL TO ART chapter — transform, never transcribe.
Output with <<<LYRICS>>>, <<<STYLE>>>, <<<TITLE>>>, <<<VOCAL>>>, and <<<END>>>
tags exactly as specified. Don't announce what you're doing — just do it.
```

- [ ] **Step 2: 编译验证(include_str! 路径)**

Run: `cargo build -p music-gift-demo 2>&1 | tail -3`
Expected: 编译通过(system.md 经 `prompts.rs` 的 `include_str!` 嵌入)

- [ ] **Step 3: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/prompts/system.md
git commit -m "feat: require material transformation in chat system prompt"
```

---

### Task 6: 前端(Elevating 事件 + 阶段态 + degraded 泛化)

**Files:**
- Modify: `examples/demo/music-gift/frontend/src/types.ts:32-35`(SseEvent 联合)
- Modify: `examples/demo/music-gift/frontend/src/components/GuidedFlow.tsx`(170-173、201、234-244、251、265-272、279、363-374、381 区域)
- Modify: `examples/demo/music-gift/frontend/src/components/ReviewCard.tsx:21-22,89-91`
- Modify: `examples/demo/music-gift/frontend/src/i18n.tsx`(5 语言:reviewing 键约在 76/228/380/532/684 行;review_skipped 键约在 89/260/431/602/773 行)

**Interfaces:**
- Consumes: Task 3 的 `SseEvent::Elevating`(wire `{"type":"Elevating"}`)、`Done.degraded` 可能含 `"elevate"`
- Produces: 无新导出;`GuidedFlow` 内部阶段态 `stage: "elevate" | "review" | null`

- [ ] **Step 1: types.ts 加 Elevating 变体**

在 `SseEvent` 联合的 `Reviewing` 成员**之前**插入:

```ts
  /** Emitted when the creative elevation pass starts (after Deltas end, before Reviewing). */
  | { type: 'Elevating' }
```

- [ ] **Step 2: GuidedFlow 状态泛化**

2a. 把 170-173 行:

```tsx
  const [review, setReview] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState(false);
  // True when the server fell back past the review pass for the current draft.
  const [reviewDegraded, setReviewDegraded] = useState(false);
```

替换为:

```tsx
  const [review, setReview] = useState<string | null>(null);
  // Which post-stream quality stage is running (elevate → review), or null.
  const [stage, setStage] = useState<"elevate" | "review" | null>(null);
  // True when any quality stage fell back to the raw draft for this turn.
  const [draftDegraded, setDraftDegraded] = useState(false);
```

2b. 201 行 `setStreaming(true); setError(null); setReviewing(false);` → `setStreaming(true); setError(null); setStage(null);`

2c. 把 234-244 行的 `} else if (e.type === "Reviewing") {` 分支整体替换为:

```tsx
        } else if (e.type === "Elevating" || e.type === "Reviewing") {
          // A post-stream quality stage (full LLM call, tens of seconds)
          // starts here — label the wait. The generation stream is over:
          // stop the reveal loop (its per-frame bottom-scroll would yank the
          // viewport past the indicator) and flush the full text now; Done
          // repeats the same flush, harmlessly.
          stopReveal();
          if (arrived) act.setMsg([...msgs, { role: "assistant", content: arrived }]);
          setStage(e.type === "Elevating" ? "elevate" : "review");
          // The indicator renders above the ReviewCard, which fills the
          // viewport — scroll to the indicator itself or the label is invisible.
          setTimeout(() => reviewingRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" }), 50);
        } else if (e.type === "Done") {
```

2d. 251 行 `setReviewDegraded(e.degraded?.includes("review") ?? false);` →

```tsx
            // Any fallen-back stage shows the same generic note.
            setDraftDegraded((e.degraded?.length ?? 0) > 0);
```

2e. finally 块(269 行)`setReviewing(false);` → `setStage(null);`

2f. handleRestart(279 行)`setInput(""); setError(null); setReview(null); setStreaming(false); setReviewing(false); setReviewDegraded(false);` → `setInput(""); setError(null); setReview(null); setStreaming(false); setStage(null); setDraftDegraded(false);`

2g. JSX(363-374 行)替换为:

```tsx
        {/* Post-stream quality stages (elevate → review): full LLM calls run
            after the stream ends. Label the wait — previously this was silent
            dead air with the input greyed out. Shown on any step: follow-up
            edits from the review screen regenerate lyrics and hit the same
            wait. */}
        {stage && (
          <div ref={reviewingRef} className="bubble bot reviewing-indicator" role="status">
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="reviewing-label">{t(stage === "elevate" ? "elevating" : "reviewing")}</span>
          </div>
        )}
```

2h. 381 行 ReviewCard 属性 `degraded={reviewDegraded}` → `degraded={draftDegraded}`

- [ ] **Step 3: ReviewCard 文案换通用 key**

3a. 21-22 行注释 `/** True when the server skipped the review pass for this draft (fallback). */` → `/** True when any lyric quality stage fell back to the raw draft. */`

3b. 89-91 行 `{t("review_skipped")}` → `{t("quality_degraded")}`

- [ ] **Step 4: i18n 五语言**

在**每个**语言字典的 `reviewing:` 一行之后加 `elevating:`,并把 `review_skipped:` 一行**替换**为 `quality_degraded:`(行号会变,以 key 名为准):

| 语言 | elevating(新增) | quality_degraded(替换 review_skipped) |
|------|----------------|---------------------------------------|
| zh | `elevating: "正在改稿润色歌词…",` | `quality_degraded: "部分歌词质量环节被跳过,已使用原始草稿",` |
| en | `elevating: "Polishing your lyrics…",` | `quality_degraded: "Some lyric quality steps were skipped; the original draft was used",` |
| fr | `elevating: "Peaufinage des paroles…",` | `quality_degraded: "Certaines étapes qualité des paroles ont été ignorées ; le brouillon original a été utilisé",` |
| es | `elevating: "Puliendo la letra…",` | `quality_degraded: "Algunos pasos de calidad de la letra se omitieron; se usó el borrador original",` |
| ru | `elevating: "Шлифую текст песни…",` | `quality_degraded: "Некоторые этапы улучшения текста были пропущены; использован исходный черновик",` |

- [ ] **Step 5: 类型检查 + 构建**

Run: `cd examples/demo/music-gift/frontend && npm run build 2>&1 | tail -6`
Expected: `tsc -b` 无类型错误,vite build 成功;`rg "review_skipped\|reviewDegraded\|setReviewing" src/ || echo clean` → `clean`

- [ ] **Step 6: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/frontend/src/types.ts examples/demo/music-gift/frontend/src/components/GuidedFlow.tsx examples/demo/music-gift/frontend/src/components/ReviewCard.tsx examples/demo/music-gift/frontend/src/i18n.tsx
git commit -m "feat: show elevate stage and generic degraded note in frontend"
```

---

### Task 7: 文档同步 + 全量验证

**Files:**
- Modify: `examples/demo/music-gift/docs/quality-gaps.md`(Closed 表 + 第 3 节)
- Modify: `examples/demo/music-gift/docs/guided-pipeline.md`(Step 6 后端清单、prompt 表、LLM 调用汇总表、涉及文件表)

**Interfaces:**
- Consumes: Task 1-6 全部落地结果
- Produces: 无代码产物

- [ ] **Step 1: quality-gaps.md**

1a. 在 "Closed (Done in feat/music-gift-demo)" 表末尾追加一行:

```markdown
| Material transformation (anti-literal) | — (no equivalent; human rewrite passes) | `prompts/elevate.md` creative elevation pass + FROM MATERIAL TO ART chapter in `skills/lyrics-writer/SKILL.md` |
```

1b. 在第 3 节 `### 3. lyric-refiner (3-Pass Tighten → Cohesion → Unity)` 的 "**Future:**" 段之后追加:

```markdown
**Status (2026-08-01):** A creative elevation pass now runs before review (`prompts/elevate.md`): it rewrites literal, on-the-nose drafts (the "transcription" failure mode) rather than tightening them. The Tighten pass described here remains a future item.
```

- [ ] **Step 2: guided-pipeline.md**

2a. Step 6 后端清单(现 102-107 行的 4-7 项)替换为:

```markdown
  4. Agent 跑完后如果输出含 `<<<LYRICS>>>`,由 `finalize_chat_output()` 依次跑两个二轮 pass(各一次完整 LLM 调用):
     1. 先发 `Elevating` 事件 → `run_text_pass(stage="elevate", prompts/elevate.md)` 创造性改稿:素材→意象转化(种子规则/单一 conceit/锚点≤2/陌生人测试/禁宣告),防"应酬诗";输出无 `<<<LYRICS>>>` 时调用点 validator 拦截并回落原稿
     2. 再发 `Reviewing` 事件 → `run_review_pass()` 用 `review.md` 做 10 点审核:自动修复发音/performance cues/artist names,标记结构/押韵/双胞胎 verse 等问题(作用于升华后的最终文本)
  5. 解析 `ParsedLyrics`,并用 `extract_review_summary()` 提取审核报告,随 `Done` 事件一并发给前端;`degraded` 数组标记回落阶段(`"elevate"` / `"review"`)
```

2b. "LLM 调用的 prompt" 表(Step 6 内)在 Review 行之后加一行:

```markdown
| Elevate | `prompts/elevate.md` | 创造性改稿:种子规则、单一 conceit、锚点≤2、陌生人测试、禁宣告;幂等(达标原样返回);输出仅标签块 |
```

并把 Lyrics 行的描述更新为:

```markdown
| Lyrics | `skills/lyrics-writer/SKILL.md` | 写词方法论(agent 按需加载):素材转化(FROM MATERIAL TO ART)、结构、押韵方案、音节、Show Don't Tell、14 点质量检查、发音修正、performance cues |
```

2c. "LLM 调用汇总" 表在 Review pass 行之后加一行:

```markdown
| Elevate pass | Chat agent 生成完歌词后(review 之前) | `elevate.md` | `chat_model`(复用) | ~10-30s |
```

2d. "涉及文件" 表:后端 `src/agent.rs` 行改为:

```markdown
| 后端 | `src/agent.rs` | `run_chat_agent` + `run_text_pass`(通用二轮 pass,elevate/review 共用) + `run_review_pass` + `finalize_chat_output` + `parse_lyrics` + `extract_review_summary`;编译期嵌入 `review.md` / `elevate.md` |
```

并在 Prompt 文件区加一行:

```markdown
| Prompt | `prompts/elevate.md` | 创造性改稿 prompt(编译期嵌入 `agent.rs`) |
```

2e. Step 6 后端小节末尾的两条 agent.rs 内联 bullet(现 107-108 行)替换为:

```markdown
- `agent.rs` — `run_chat_agent()`:Orchest AgentRun,max 5 steps
- `agent.rs` — `run_text_pass()`(通用单步二轮 AgentRun,elevate/review 共用) + `run_review_pass()` + `finalize_chat_output()`(编排:门控 → Elevating → elevate → Reviewing → review);`elevate.md` / `review.md` 编译期 `include_str!` 嵌入,无 tool
```

- [ ] **Step 3: 全量验证**

Run: `cargo fmt -p music-gift-demo && cargo clippy -p music-gift-demo --all-targets -- -D warnings && cargo test -p music-gift-demo 2>&1 | tail -4`
Expected: fmt 无 diff、clippy 零警告、81 passed + smoke 2 passed

Run: `cd examples/demo/music-gift/frontend && npm run build 2>&1 | tail -3`
Expected: 构建成功

泛化走查(对照 Global Constraints):
Run: `rg -n "谷粒|小脚丫|chinese|zh\b" examples/demo/music-gift/src/agent.rs examples/demo/music-gift/src/routes.rs || echo "no special-casing"`
Expected: `no special-casing`(质量规则只在 prompt/skill 文件,Rust 无特判)

- [ ] **Step 4: 人工验证(用户执行,agent 提供步骤)**

1. `cd examples/demo/music-gift && ./scripts/serve.sh restart`
2. 用原素材重新走一遍引导流程:孩子(谷粒)/日常/想留住这一刻
3. 对照规格验收标准:名字不进 chorus(≤1 次)、无逐动作复述、无情绪宣告句、存在贯穿 conceit;聊天 UI 依次出现"正在改稿润色歌词…"→"正在检查歌词质量…"两个等待提示

- [ ] **Step 5: Commit(先获用户确认)**

```bash
git add examples/demo/music-gift/docs/quality-gaps.md examples/demo/music-gift/docs/guided-pipeline.md
git commit -m "docs: sync quality-gaps and guided-pipeline with elevate pass"
```

---

## 完成定义(对照规格验收标准)

- [ ] 同素材重新生成:名字不再进 chorus(≤1 次)、无逐动作复述、无情绪宣告句、存在贯穿 conceit
- [ ] elevate 失败时链路回落原稿,前端 degraded 提示可见(任一阶段)
- [ ] elevate 输出丢 `<<<LYRICS>>>` 时 validator 拦截、回落原稿(单测覆盖)
- [ ] `cargo test -p music-gift-demo` 全绿,clippy 零警告
- [ ] 新增 Rust 代码无任何歌词/语言/人名特判(泛化走查通过)
