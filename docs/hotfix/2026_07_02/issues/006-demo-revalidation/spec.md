# Issue 006:Briefing Desk demo 重验证 + 报告/路线图收尾

GitHub: 实施开始时新建(#195–#199 已存在,本条是 hotfix 自身的收尾 issue) · 依赖 001–005

## 目的

v0.10 验证报告与各 blocker issue 的验收标准都要求:修复后**重跑 demo 并贴输出**,code-only 修复不得关闭。001–005 各自跑过局部验证;本条做一次全量收尾,防止相互作用遗漏(001 改了 `start` 签名、002/003 改了 resume 面、004 改了 demo 的 `reviewer_tool()`、005 换了 demo 的 media 基建——五处都汇聚在同一个 demo)。

## 范围

1. 全量重跑并留档:
   - `cargo test -p briefing-desk-demo`(19 个自动化测试全绿)
   - `--fake` 模式手动完整 run 一次(approval 流、session persist + resume、describe_image 真实 Image block 路径)
   - live 模式(env-var gated)至少验证一次真实 Anthropic 视觉调用,兑现报告"未完成 live provider 验证前不得推进 v1.0"的要求
2. `docs/review/v0_10_demo_validation.md`:
   - Triage 表 5 行 blocker 标注已修复,链接关闭它们的 commit/PR
   - Freeze Coverage Statement 的 "Multimodal image input" 行更新
3. `docs/iteration/roadmap.md`:能力缺口表"多模态图片输入"行从 release blocker 改为已完成;"已完成"表补本 hotfix 一行
4. `examples/demo/briefing-desk/README.md` 若受 001/005 影响则同步

## 验收标准

- [ ] 三种 run(自动化 / `--fake` / live 视觉)输出贴回对应 issue 或 PR
- [ ] 验证报告、roadmap、demo README 全部同步
- [ ] `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh` 全过(hotfix 合并前的最终门)
