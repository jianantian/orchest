# Issue 002:`AgentRun::resume` 支持追加新输入

GitHub: [#197](https://github.com/jianantian/orchest/issues/197) · release-blocker · 依赖 issue 001(复用 `RunInput`)

## 现状

`AgentRun::resume(snapshot, model, registry)`(`crates/orchest/src/run/mod.rs:79-107`)没有 input 参数。带新问题续会话必须手动向 `SessionSnapshot.messages` push 一条 user message——不读源码不可能发现;跳过这步不报错,模型对着未变的历史被重新调用,产出过期/重复回答(silent-wrong-behavior)。doc comment 只有一句 "Resume a previous run from a persisted snapshot.",未提此坑。

## 方向(提案)

**保留 `resume` 原语义 + 新增 `resume_with_input`**,复用 001 的 `RunInput`:

```rust
/// 从快照原样恢复(中断恢复场景:历史不变,继续未完成的 run)。
pub fn resume(snapshot, model, registry) -> ...          // 现状保留,doc 补坑说明

/// 从快照恢复并追加一个新的 user turn(follow-up 场景)。
pub fn resume_with_input(
    snapshot: SessionSnapshot,
    input: impl Into<RunInput>,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
) -> (RunHandle, EventReceiver)
```

两个入口语义正交:`resume` = "继续被打断的 run"(不该塞新输入),`resume_with_input` = "带新问题续会话"。比给 `resume` 加 `Option<RunInput>` 参数清晰——`None` 和空输入的边界不需要调用方猜。实现上 `resume_with_input` 即在 `ResumeState.messages` 尾部 push `Message { role: User, content: input.into_blocks() }` 后走同一条 spawn 路径。

同时落 #197 的文档修复项(即验证报告 Triage #10):

- `resume` 的 rustdoc 明确"本函数不追加输入;follow-up 用 `resume_with_input`"
- `docs/guide/quickstart.md` §8 的 `session_persist_resume.rs` 指引补一段说明

## 落地与测试

- `examples/demo/briefing-desk/src/app.rs` 的 `resume()` 删掉手动 push,改用 `resume_with_input`
- 测试:`resume_with_input` 后发给模型的 messages 末尾是新 user turn(fake adapter 捕获);`resume` 行为不变的回归测试
- 重跑 `cargo test -p briefing-desk-demo`(特别是 `session_persists_across_processes_and_resume_references_original_brief`),输出贴回 #197 或关闭它的 PR

## 验收标准(对齐 GitHub #197)

- [ ] `resume_with_input` 落地,demo `resume()` 改用它
- [ ] `resume` doc comment 与 quickstart §8 补坑说明
- [ ] demo 测试重跑,输出贴回 issue/PR
- [ ] `docs/review/v0_10_demo_validation.md` 更新(Triage #3、#10 两行)
