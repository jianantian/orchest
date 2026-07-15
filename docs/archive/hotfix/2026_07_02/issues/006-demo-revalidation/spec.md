# Issue 006:Briefing Desk demo 重验证 + 报告/路线图收尾

GitHub: [#201](https://github.com/jianantian/orchest/issues/201) · 依赖 001–005(#195–#199,全部已关闭)

## 目的

v0.10 验证报告与各 blocker issue 的验收标准都要求:修复后**重跑 demo 并贴输出**,code-only 修复不得关闭。001–005 各自跑过局部验证;本条做一次全量收尾,防止相互作用遗漏(001 改了 `start` 签名、002/003 改了 resume 面、004 改了 demo 的 `reviewer_tool()`、005 换了 demo 的 media 基建——五处都汇聚在同一个 demo)。

## 范围

1. 全量重跑并留档:
   - `cargo test -p briefing-desk-demo` 全绿(001/002/004/005 各自会新增测试,不钉死具体条数)
   - `--fake` 模式手动完整 run 一次(approval 流、session persist + resume、describe_image 真实 Image block 路径)
   - live 模式(env-var gated)至少验证一次真实 Anthropic 视觉调用,兑现报告"未完成 live provider 验证前不得推进 v1.0"的要求
2. `docs/review/v0_10_demo_validation.md`:
   - Triage 表 5 行 blocker 标注已修复,链接关闭它们的 commit/PR
   - Freeze Coverage Statement 的 "Multimodal image input" 行更新
3. `docs/iteration/roadmap.md`:能力缺口表"多模态图片输入"行从 release blocker 改为已完成;"已完成"表补本 hotfix 一行
4. `examples/demo/briefing-desk/README.md` 若受 001/005 影响则同步

## 验收标准

- [x] 自动化 / `--fake` 两种 run 输出贴回本 issue(见下方"实现记录");**live 视觉验证未做**——见下方说明,这是已知的、有意标注的缺口,不是遗漏
- [x] 验证报告、roadmap、demo README 全部同步
- [x] `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh` 全过(hotfix 合并前的最终门)

## live 视觉验证:未完成,已知缺口

本次实施环境没有 `ANTHROPIC_API_KEY`(或任何 `BRIEFING_DESK_*_API_KEY`)、
也没有面向 Anthropic API 的出网权限,因此**无法**在本次收尾中补上验证报告
自己标注的"live provider 验证"缺口(`docs/review/v0_10_demo_validation.md`
的"Live provider run"一节)。这与 v0.10 原始验证报告的结论一致——该报告
从一开始就写明"本环境无凭证,该验证必须由有凭证的维护者手动补做",本次
收尾没有改变这个事实,只是把它从"待办"重申为"hotfix 2026-07-02 仍待办"。

`docs/review/v0_10_demo_validation.md` 与 `docs/iteration/roadmap.md` 均已
更新,明确记录:v1.0 在这项 live 验证补上之前不得仅凭本报告推进。补做方式
见验证报告"Live provider run"一节列出的命令(`BRIEFING_DESK_ASR_*`/
`BRIEFING_DESK_TTS_*` 环境变量,加 `examples/rust/multimodal_image_input.rs`
配一个真实可访问的图片 URL 和 `ANTHROPIC_API_KEY`)。

## 实现记录

- 新建 GitHub 收尾 issue [#201](https://github.com/jianantian/orchest/issues/201),PRD 与本 spec 头部同步补链接
- 自动化重跑:`cargo test -p briefing-desk-demo` 20/20 全绿(commit `8bb9a9b74f132f9e8db8464819d9364f25595fcd`,#195–#199 全部合并后)
- 手动 `--fake` 全流程重跑(独立于测试二进制的真实 CLI session):
  - `run`:`search_fixtures` → `read_fixture` → `transcribe_audio` → `describe_image`(真实 `ContentBlock::Image` 路径,#195)→ `review_report`(Agent-as-Tool 子 run)→ `write_report`(approval 通过)→ `synthesize_brief`(approval 通过),exit 0,brief.md/brief.wav 均正确写出
  - `resume`(独立进程,`--session demo-session-006`):经 `resume_with_input`(#197)+ session_store 重挂(#198)成功续会话,`run_id` 与原 run 一致,follow-up 回答引用原 brief 内容
  - 完整输出见 `docs/review/v0_10_demo_validation.md` "Runs" 一节
- 文档同步:
  - `docs/review/v0_10_demo_validation.md`:Triage 表 5 行全部标记 Fixed 并带 commit 链接;Freeze Coverage Statement 的 ASR/TTS/多模态图片输入三行更新;"Release-blocker fixes required before v1.0" 一节 4 项全部标记已完成(保留历史记录);顶部加 hotfix 收尾说明段落;"Runs" 一节补充 hotfix 后的测试/手动 run 记录
  - `docs/iteration/roadmap.md`:能力缺口表"多模态图片输入"行改为 ✅ 已完成;"已完成"表新增 hotfix 2026-07-02 行;v1.0 依赖段落更新,明确 5 个 blocker 已清偿、live provider 验证仍未做
  - `examples/demo/briefing-desk/README.md`:fake smoke 测试说明改为指向 `orchest_provider::fakes`(#196);Session persistence and resume 一节补充 `resume_with_input`/`SessionStoreMissing` 说明(#197/#198)
- `cargo test --workspace --features orchest/sqlite-session` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh`:全部通过,与 001-005 记录的基线一致(同样两处既有、范围外的 clippy finding)
