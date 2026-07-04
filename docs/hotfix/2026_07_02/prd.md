# Hotfix 2026-07-02 PRD:v0.10 验证报告 Release Blocker 清偿

## 背景

v0.10 Demo A(Briefing Desk)验证报告([`docs/review/v0_10_demo_validation.md`](../../review/v0_10_demo_validation.md))产出 5 项 release blocker,均已建独立 GitHub issue([#195](https://github.com/jianantian/orchest/issues/195)–[#199](https://github.com/jianantian/orchest/issues/199))。路线图明确:v1.0 发布前必须清偿这 5 项,且"修复后必须重跑 Briefing Desk demo 验证",code-only 修复不算关闭。

本 hotfix 一次性清偿全部 5 项,外加一个收尾 issue(demo 重验证 + 验证报告/路线图更新)。

## 目标

1. 关闭 #195–#199 全部 5 个 release-blocker issue
2. 每项修复后按各 issue 验收标准重跑 `cargo test -p briefing-desk-demo`(以及 `--fake` 手动 run),输出贴回 issue 或关闭它的 PR
3. `docs/review/v0_10_demo_validation.md` 的 Triage 表与 Freeze Coverage Statement 同步更新
4. 公开 API 变更(#195/#197/#198/#199 都动公开面)保持相互一致:#195 引入的 `RunInput` 类型被 #197 复用,不各造一套

## 成功指标

- 5 个 GitHub issue 全部关闭,每个 issue 的验收 checklist 全勾
- `ContentBlock::Image` 经公开 API 到达真实 `ModelAdapter::complete()` 的路径存在且有测试/示例覆盖(v0.10 报告的最大发现被消除)
- `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、`bash scripts/lint-check.sh` 全过
- 验证报告 Freeze Coverage Statement 中 "Multimodal image input" 一行从 blocker 变为 covered

## Issue 拆分

| Issue | 标题 | GitHub | 依赖 |
|-------|------|--------|------|
| 001 | 多模态图片输入公开 API(`RunInput`) | [#195](https://github.com/jianantian/orchest/issues/195) | 无(先行,设计见 [spec](./issues/001-multimodal-image-input/spec.md)) |
| 002 | `AgentRun::resume` 支持追加新输入 | [#197](https://github.com/jianantian/orchest/issues/197) | 001(复用 `RunInput`) |
| 003 | 反序列化 `AgentConfig` 丢 session store 时响亮失败 | [#198](https://github.com/jianantian/orchest/issues/198) | 002(共用 resume 面,顺序改动避免冲突) |
| 004 | `SubAgentBuilder::build()` 返回 `Result` | [#199](https://github.com/jianantian/orchest/issues/199) | 无 |
| 005 | `orchest-provider` 提供可复用 fake `Asr`/`Tts` | [#196](https://github.com/jianantian/orchest/issues/196) | 无 |
| 006 | Briefing Desk demo 重验证 + 报告/路线图收尾 | (实施时新建) | 001–005 |

依赖顺序:001 → 002 → 003;004、005 独立,可穿插;006 收尾。

按 WORKFLOW:实施分支 `hotfix/2026_07_02`,一 issue 一 commit,commit message 带 `closes #N`。006 的 GitHub issue 在实施开始时补建(#195–#199 已存在,无需重建)。

## 范围裁定(全 hotfix 级)

- **#195 只修入口方向**:user turn 能携带 `ContentBlock::Image` 进 agent loop。`ToolResult.content: Value` 的类型改造(工具向下一轮注入图片)**不做**,理由与替代路径见 001 spec 的"非目标"
- **不动 provider 序列化层**:Anthropic/Minimax adapter 已能序列化 `ContentBlock::Image`(`orchest-provider-http` 的 `providers/{anthropic,minimax}/request.rs`),本次只打通 runtime 入口
- **Python/TS SDK 的多模态入参暴露不做**:`AgentRun::start` 改用具体类型 `RunInput` 后(见 001 spec 决策 1 的 rustc 实测,推翻了最初 `impl Into<RunInput>` 的方案),两个 binding 各自的调用点补一行 `RunInput::text(input)` 即可继续走纯文本;binding 层暴露 blocks 参数是 post-hotfix 工作
- **post-1.0 backlog 项(验证报告 Triage #6–#9)不捎带**,保持 hotfix 聚焦

## 验收标准

- [ ] 001–005 各自 spec 的验收 checklist 全过
- [ ] #195–#199 全部由 `closes #N` commit 关闭
- [ ] `docs/review/v0_10_demo_validation.md` Triage 表 5 行 blocker 标注已修复(含 PR/commit 链接),Freeze Coverage Statement 更新
- [ ] `docs/iteration/roadmap.md` 能力缺口表"多模态图片输入"行更新为已完成
- [ ] `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh` 全过

## 依赖

- 无外部依赖。全部改动在 `crates/orchest`、`crates/orchest-provider`、`examples/demo/briefing-desk` 与 docs 内
- 不阻塞 v0.11 规划;v1.0 依赖本 hotfix 完成
