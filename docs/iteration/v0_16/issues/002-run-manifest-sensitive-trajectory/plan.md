# 002 — 实施计划

## 要读的文件

- `docs/iteration/v0_16/prd.md`、001 的 `spec.md`/`plan.md` 与本目录 `spec.md`
- `examples/demo/briefing-desk/src/harness.rs`、`src/eval/case.rs`（001 的 surface/corpus 合同）
- `examples/demo/briefing-desk/src/app.rs`（`drain_events`、run/resume/session store）
- `examples/demo/briefing-desk/src/media.rs`（`DescribeImageTool` 内部 vision model call）
- `crates/orchest/src/events.rs`（完整 `RuntimeEvent` 枚举）
- `crates/orchest-protocol/src/stream.rs`、`src/types.rs`、`src/response.rs`
  （Thinking/provider details、ContentBlock、TokenUsage）
- `crates/orchest/src/session/{snapshot,store,sqlite}.rs`（snapshot/store 生命周期）
- `crates/orchest/src/tool/mod.rs`、`crates/orchest/src/budget.rs`
  （`ToolOutput::Structured.external_usage` 与 budget 口径）

## 要改的文件

- `examples/demo/briefing-desk/Cargo.toml`（SHA-256、runtime tempdir 等依赖）
- `examples/demo/briefing-desk/src/eval/mod.rs`
- `examples/demo/briefing-desk/src/eval/trajectory.rs`
  （`TrajectoryEvent`、递归 sanitizer、JSONL writer）
- `examples/demo/briefing-desk/src/eval/artifact.rs`
  （manifest、harness snapshot、attempt 四文件与不可覆盖 writer）
- `examples/demo/briefing-desk/src/eval/session.rs`
  （seed materialize、独立 SQLite store、cleanup guard）
- `examples/demo/briefing-desk/src/app.rs`
  （不改变普通 CLI 的可选 event observer/captured-run seam）
- `examples/demo/briefing-desk/src/media.rs`
  （vision usage 通过 event-only details 与 `external_usage` 上报）
- `examples/demo/briefing-desk/.gitignore`、`README.md`
- `examples/demo/briefing-desk/src/eval/` 内单元测试与测试 fixtures

## 步骤

1. 先用合成 `RuntimeEvent` 写 sanitizer 测试：顶层及 `ChildRunEvent`/`SubAgentEvent` 内的
   `Thinking`、`ThinkingEnd.signature/provider_details`、全部其他 `ModelStreamChunk` 和
   `SubAgentStarted.config_summary` 必须不产生 payload；Tool/approval/terminal Values 中的
   reasoning 与 secret-key 变体必须删除或遮盖。
2. 定义 `TrajectoryEvent { schema_version, sequence, elapsed_ms, run_relation, kind, data }`；
   对每个 `RuntimeEvent` variant 做穷举 allowlist 映射，child wrappers 递归调用同一 sanitizer，
   `EventsDropped` 同时设置 attempt inconclusive 标记。
3. 为 sanitizer 的保留路径补测试：Tool 名/顺序、approval context、usage、stop reason、child
   关系与 terminal output 可用于 grader；sequence 严格递增，JSONL 每行可独立解析。
4. 定义规范化 `HarnessSnapshot`：按 `surface_id` 排序、CRLF 转 LF、不 trim，落盘准确文本和
   SHA-256；测试从 snapshot 恢复 surfaces、单字符改动改变 hash、manifest path/hash 一致。
5. 实现 git/manifest preflight：记录 commit、dirty bool/paths、fixture revision、非秘密 model
   options、surface/seed hashes、case/repetition/schema；harness 外 dirty path、secret 字段、
   label 已存在或 snapshot/hash 不一致时拒绝创建 run。测试改变 API key 环境变量不会改变
   snapshot/manifest 非秘密字段，且秘密值不落盘。
6. 实现 artifact transaction：先创建新 label 目录，attempt 无论 completed、
   `execution_failure` 或 `inconclusive` 都原子写出 `trajectory.jsonl`、`output.md`、
   `attempt.json`、`scores.json`；失败评分文件写 `grader_status: not_run` 与 null aggregate。
7. 实现 `AttemptSession` guard：从已校验 seed materialize 唯一 ID 的 `SessionSnapshot`，为每个
   attempt 创建独立临时 SQLite store并 re-attach；artifact flush 后 delete/drop tempdir，
   cleanup 结果进入 `attempt.json`。测试前一 attempt 的 mutation 在后一 attempt 不可见，并
   覆盖成功、执行失败与 cleanup 失败。
8. 把 `app::drain_events` 重构为普通 CLI 可用的 captured-run seam：recorder 在 stdout match
   之前观察事件；普通 `run`/`resume` 传 no-op observer，现有行为与输出保持不变。
9. 将 `DescribeImageTool` 改为返回相同 `model_output` 的 `ToolOutput::Structured`，在 event-only
   details 保存完整 `TokenUsage`，并用 `external_usage` 让 parent budget 计入 input+output；
   补测试证明模型可见结果不变、eval 能读取 vision usage。
10. 更新 `.gitignore` 与 README，说明 `evals/runs/` 的敏感性、默认本地保留/删除方式、禁止
    提交与“sanitized 不等于非敏感”。
11. 运行 demo tests 与 workspace test/clippy/fmt/lint-check。
