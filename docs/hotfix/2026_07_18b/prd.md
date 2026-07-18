# Hotfix 2026_07_18b — Provider 请求正确性 + run 终止语义

> 来源: [`docs/todo/2026-07-18-sdk-optimization-plan.md`](../../todo/2026-07-18-sdk-optimization-plan.md) 的 hotfix 档(C1 / C2 / B3)。
> 同日已有 hotfix/2026_07_18(ASR SegmentRef,PR #213)合入,本 hotfix 为当日第二个,命名加 `b` 后缀。

## 背景

music-gift demo 生成质量梳理发现三处"全使用方静默中招"的 SDK 缺陷,均小、独立、修复收益立竿见影:

1. **cache_control 断点位置**: Anthropic Messages 请求把 `cache_control` 写在 body 顶层。按现行 vendor 文档(`docs/external/anthropic/api.md` Create a Message 参数表:顶层 `cache_control` "automatically applies a cache_control marker to the last cacheable block"),顶层写法在 Anthropic 侧**合法**——真实动机是:(a) 顶层写法把断点位置交给 API 决定,无法显式控制;(b) Anthropic-compatible 端点(如 Minimax,`docs/external/minimax/llm/activate_cache.md`)只文档化 block 级形态,顶层字段在这些端点行为未定义。(更正记录:初稿称"顶层不合法、缓存从未生效、严格端点 400",经 code review 对照 `api.md:1149-1151` 证伪。)
2. **thinking budget 与 max_tokens 默认组合非法**: `RequestOptions` 默认 thinking=Medium(非 adaptive 模型映射 `budget_tokens=10240`),适配器 max_tokens 默认 4096;Anthropic 要求 max_tokens > budget_tokens → 默认配置 + 非 adaptive 模型直接 400。
3. **异常 stop_reason 空转**: 模型返回非 EndTurn/MaxTokens 的 stop_reason 且无 tool_use 时,run 不失败,反而 push 一条空 content 的 User 消息,用相同上下文反复调用直到 max_steps(默认 100)耗尽——烧 token 后仍失败。

**不在范围**: SDK-0(`crates/orchest-provider-core/src/gen.rs` 测试编译失败)系 feat/music-gift-demo 分支本地问题(TimedText commit `3cc248a` 遗漏,未上 main),在该分支单独修复,不属于本 hotfix。

## 范围与依赖顺序

| # | Issue | 文件 |
|---|-------|------|
| 001 | cache_control 移到 content block 级 | `issues/001-cache-control-block-level/spec.md` |
| 002 | thinking budget 与 max_tokens 组合合法性 | `issues/002-thinking-budget-max-tokens/spec.md` |
| 003 | 异常 stop_reason 终止 run 而非空消息死循环 | `issues/003-abnormal-stop-reason-fails-run/spec.md` |

三项互相独立,按编号顺序提交(一项一 commit)。

## 验收

- `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `cargo fmt --check` / `bash scripts/lint-check.sh` 全绿
- 各 issue spec.md 验收框全勾

## Out of scope

- cache breakpoint 多断点策略(本 hotfix 只做单一合理断点的位置调整)
- 模型重试默认策略(SDK-C4)、上下文管理默认值(SDK-C3)——留后续迭代
- `MaxTokens` 截断标记(SDK-B2)——留后续迭代,003 只处理异常 stop_reason
