# v0.13 PRD: 生成质量地基(Generation Quality Foundation)

## 背景

music-gift demo(见 `examples/demo/music-gift/docs/optimization-plan.md` 与 `docs/todo/2026-07-18-sdk-optimization-plan.md`)暴露出一组"所有使用方共享"的生成质量地基缺陷。hotfix 2026_07_18b 已清偿最急的三项(cache_control 位置、thinking budget 组合、异常 stop_reason 空转)。本迭代清偿下一档——它们直接决定"精心构造的 prompt 能否完整、可靠地到达模型,以及模型中断时 run 是否死得明白":

1. **输入契约**(SDK-A1): `AgentRun` 公开 API 只能以单 user turn 启动,多轮历史只能拍平成一条 user 消息——demo 因此丢掉 system 角色与历史结构,是歌词质量的最大单因。
2. **截断静默**(SDK-B2): `MaxTokens` 截断被当作正常完成,消费方无法区分"完整"与"被砍断"。
3. **上下文管理缺位**(SDK-C3): catalog 的 `context_window` 不回填 `ModelSpec`,默认配置下上下文无限增长直到 provider 400。
4. **重试缺位**(SDK-C4): 模型重试默认关、SSE 流中断归类 NoRetry,部分内容丢弃即 RunFailed。

## 目标

1. 使用方能以完整角色结构(system prompt + 多轮历史)启动 run,wire 上 system 非空、历史角色边界保留。
2. 截断完成与完整完成在事件层可区分。
3. 经 registry 创建的模型自带 `context_window_size`,上下文硬校验/compaction 可用。
4. 流中断可重试,一行配置即可开启推荐的模型重试策略。

## 非目标

- 不改 compaction 的默认开关(保持 opt-in);不做压缩策略调优。
- 不做 Gen 协议参数类型化(SDK-B1)、skill 披露机制(SDK-D1)——属后续迭代。
- 不为 001 重构 demo;demo 去拍平(optimization-plan D6)在 v0.13 完成后单独跟进。
- 不改变 `AgentRun::start` / `RunInput` 现有行为与签名(只新增,不破坏)。

## Issue 分解

| Issue | 标题 | 来源 |
|-------|------|------|
| 001 | AgentRun 公开多轮启动入口 | SDK-A1 |
| 002 | MaxTokens 截断标记 | SDK-B2 |
| 003 | catalog context_window 回填 ModelSpec | SDK-C3 |
| 004 | 流中断可重试 + 一行重试配置 | SDK-C4 |

依赖顺序: 001 → 002 → 003 → 004(相互基本独立,按编号顺序提交,一 issue 一 commit)。

## 验收

- `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh` 全绿
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` 通过(CI 新增口径,见 #218 教训)
- 各 issue spec.md 验收框全勾
- music-gift 去拍平(D6)在 001 落地后无 SDK 侧阻塞(在 demo 分支验证,不进本迭代)

## 依赖

- hotfix 2026_07_18b 已合入(002/004 复用其 stop_reason 分支与 retry 结构)
- 与 v0.11(Demo B)无文件级冲突,可并行
