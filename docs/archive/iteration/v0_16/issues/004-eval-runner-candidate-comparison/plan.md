# 004 — 实施计划

## 要读的文件

- `docs/iteration/v0_16/prd.md`、001–003 的 `spec.md`/`plan.md` 与本目录 `spec.md`
- `examples/demo/briefing-desk/src/main.rs`（clap subcommand 结构）
- `examples/demo/briefing-desk/src/app.rs`（captured-run seam、approval、run/resume）
- `examples/demo/briefing-desk/src/eval/{case,artifact,trajectory,session}.rs`
- `examples/demo/briefing-desk/src/eval/grader/`（评分与聚合 API）
- `examples/demo/briefing-desk/tests/smoke.rs`（CLI integration test 惯例）
- `crates/orchest/src/run/{mod,handle}.rs`（start/resume、terminal、wait/approval API）
- `crates/orchest/src/budget.rs`、`crates/orchest-protocol/src/{response,options}.rs`
  （budget、reasoning usage 与 pricing 的非重复计数口径）

## 要改的文件

- `examples/demo/briefing-desk/src/main.rs`（`eval run`/`eval compare` CLI）
- `examples/demo/briefing-desk/src/app.rs`
  （runner 使用的 model-injected/captured execution 入口）
- `examples/demo/briefing-desk/src/eval/mod.rs`
- `examples/demo/briefing-desk/src/eval/cli.rs`（参数、preflight、退出状态）
- `examples/demo/briefing-desk/src/eval/runner.rs`
  （split/repetition、attempt 生命周期、计时、usage、grading）
- `examples/demo/briefing-desk/src/eval/compare.rs`
  （可比性、baseline validity、candidate gates、JSON/Markdown）
- `examples/demo/briefing-desk/README.md`（命令、sealed/失封流程）
- `examples/demo/briefing-desk/tests/eval_cli.rs`
  （scripted/fake model 端到端测试）

## 步骤

1. 先为 CLI preflight 写 integration tests：缺 `--record-sensitive`、未知 split、invalid corpus、
   label 冲突、harness 外 dirty path和 scorecard 缺 `--confirm-sealed` 都必须在 model call 前
   非零退出且不创建半成品 run。
2. 增加 `eval run` 参数：显式 label、一个或多个 split、敏感确认与 sealed 确认；optimization/
   validation/scorecard 默认 repetitions 固定为 1/3/3，并写入 manifest。
3. 把 app 内部执行入口参数化为 `Arc<dyn ModelAdapter>`、case input、registry/session 和 event
   observer；普通 CLI 仍从环境构造 model，runner tests 注入 scripted model，不复制产品 run
   pipeline。
4. 实现 attempt executor：fresh 直接 start，follow-up 使用 002 的独立 seed/store 后
   `resume_with_input`；用 monotonic `Instant` 从调用前计至 terminal 已收且 handle wait 返回，
   自动处理 case 声明的 approval 决策并把 run 内处理时间计入 wall latency。
5. 单个 attempt 总是 finalize 四文件；execution failure/inconclusive 不停止后续 case。
   `EventsDropped`、缺 terminal、stream 提前关闭、cleanup 失败或 grader error 都保持独立状态。
6. 实现 resource collector：递归收集 normal/child `ModelCallCompleted` 和 vision Tool 的
   event-only usage，分别求和 token 字段；`gate_total_tokens` 只加 input/output/
   audio/image/video input，reasoning/cache/details 单列。测试 output 已包含 reasoning 细分时
   不重复计数。已知内部 call 缺 usage 时标记 coverage incomplete。
7. 运行 003 graders 并生成 attempt、case、tag、split 结果；若必需 attempt 未 completed，不
   生成缩小分母的 aggregate。
8. 先写 compare table tests，再实现 manifest comparability：effective-config hash、commit、
   fixture、case/split/repetitions、session seed hashes 与非-harness source 必须一致，并一次
   列出全部 mismatch。单独测试 runtime/retry/approval/Tool schema/capability route/session mode
   变化被拒绝，而 harness snapshot hash 变化被允许。compare 从磁盘重算 snapshot hashes；
   篡改文件/hash 时拒绝，配置不同时输出字段级 diff。
9. 实现 baseline validity：任一 baseline must-pass attempt 失败返回 `invalid_baseline`，保存
   case/attempt evidence，不再计算 candidate eligibility。
10. 实现 candidate 独立 gates：全部 must-pass attempts 绝对通过、validation weighted score
    至少 +5、每个 validation tag 不下降、validation attempt mean gate tokens ≤115%、median
    wall latency ≤130%、全部 completed/graded/resource complete。偶数 median 取中间两项平均。
11. 输出稳定 schema 的 compare JSON 和 Markdown；最终状态只有 `invalid_baseline`、
    `not_eligible` 或 `eligible_for_review`，逐项列出实际值/阈值，不编辑 harness、不删除 run。
12. 补 scripted CLI 端到端测试：成功 artifacts、继续执行其他 case、每个独立 gate、共同
    must-pass 失败、失败 latency 不进入 median但完整性失败、vision usage 缺失、sealed manifest。
13. 更新 README 的 baseline/candidate/scorecard 命令、人工审核与 scorecard 查看后失封语义。
14. 运行 demo tests 与 workspace test/clippy/fmt/lint-check。
