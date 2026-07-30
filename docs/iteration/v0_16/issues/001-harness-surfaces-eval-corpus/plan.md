# 001 — 实施计划

## 要读的文件

- `docs/iteration/v0_16/prd.md` 与本目录 `spec.md`（surface、case、split、seed 合同）
- `examples/demo/briefing-desk/src/main.rs`（现有 clap 入口与模块边界）
- `examples/demo/briefing-desk/src/app.rs`（主/reviewer prompt、Tool 注册与 run/resume）
- `examples/demo/briefing-desk/src/tools.rs`、`src/media.rs`（全部 Tool descriptions）
- `examples/demo/briefing-desk/tests/smoke.rs`（demo 行为回归）
- `examples/demo/briefing-desk/fixtures/`（可引用材料 inventory）
- `crates/orchest/src/session/snapshot.rs`、`crates/orchest/src/run/config.rs`
  （seed materialize 所需字段与 serde 边界）
- `crates/orchest-protocol/src/types.rs`（版本化 seed 中 Message/ContentBlock 的数据形态）

## 要改的文件

- `examples/demo/briefing-desk/Cargo.toml`（case/seed serde 依赖）
- `examples/demo/briefing-desk/src/main.rs`（注册 `harness`、`eval` 模块）
- `examples/demo/briefing-desk/src/harness.rs`（唯一 prompt/description 定义）
- `examples/demo/briefing-desk/src/app.rs`、`src/tools.rs`、`src/media.rs`
  （只引用 harness 常量，删除重复字符串）
- `examples/demo/briefing-desk/src/eval/mod.rs`、`src/eval/case.rs`
  （case/seed 类型、加载器与 validator）
- `examples/demo/briefing-desk/evals/cases.json`（18 个版本化案例）
- `examples/demo/briefing-desk/evals/session-seeds/*.json`（合成 follow-up 前置状态）
- `examples/demo/briefing-desk/tests/smoke.rs` 与 `src/eval/case.rs` 单元测试
- `examples/demo/briefing-desk/README.md`（candidate surface 与 corpus 说明）

## 步骤

1. 先为 `CaseCorpus::validate()` 写失败测试：重复 ID、非法/缺失 split、scenario family 跨
   split、未知/无 validation 覆盖 tag、非正权重、空 expected、越界 fixture、缺失/非法
   session seed 都必须在 model call 前返回含 case ID/字段名的错误。
2. 在 `harness.rs` 定义稳定 `surface_id` 和文本常量；把主 prompt、reviewer prompt、
   reviewer Tool description 与所有 in-process Tool descriptions 迁入，保持 schema、metadata、
   execute 与现有 `run`/`resume` 输出语义不变。
3. 实现 serde case 类型：`EvalSplit`、`RunMode::{Fresh, FollowUp}`、行为标签、grader/Tool/order/
   fact/report/fixture 合同和 case weight；拒绝 schema version 不支持的 corpus。
4. 定义 `SessionSeed`：只允许 schema version、messages、step、budget usage；规范化 JSON 后
   计算内容 hash，拒绝 session/run ID、store path 或 harness config 等 mutable 字段。
5. 编写 10 optimization、4 validation、4 scorecard cases；scenario family 不跨 split，4 个
   validation cases 合计覆盖全部七个 gating tags，follow-up cases 只引用已提交 seed。
6. 添加正向测试，直接加载仓库内完整 corpus、fixture inventory 和 seeds，断言数量、标签覆盖、
   split 隔离、引用与 seed hash 全部有效；保留第 1 步的逐类负例。
7. 更新 README，明确唯一 candidate surface、18-case Loom 边界和 seed 是合成只读前置状态。
8. 运行 `cargo test -p briefing-desk-demo`，再运行 workspace test/clippy/fmt 与
   `bash scripts/lint-check.sh`。
