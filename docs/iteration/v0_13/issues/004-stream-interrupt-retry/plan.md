# 004 — 实施计划

## 要读的文件

- `crates/orchest/src/run/retry.rs`(分类逻辑与 RetryPolicy 结构)
- `crates/orchest-provider-http/src/sse/mod.rs:300-302` 附近(stream_error/stream_interrupted 的产生)
- `crates/orchest/src/run/actor.rs`(`handle_model_error_or_retry` 的调用路径)
- `crates/orchest/src/run/config.rs:517` 附近(retry_policy 配置面)
- `crates/orchest-py` / `crates/orchest-node`(绑定层 retry 硬编码 None 处)

## 要改的文件

- `crates/orchest/src/run/retry.rs`(分类 + 推荐配置构造)
- `crates/orchest-provider-http/src/sse/mod.rs`(如需为错误补充分类所需信息)
- `crates/orchest/src/run/config.rs`(builder 入口,如需要)
- `crates/orchest-py` / `crates/orchest-node`(绑定透传)
- 测试

## 步骤

1. 确认流中断错误的现有形状(code/kind/status),在 classify 中把网络层流中断归为可重试(注意区分协议级错误,如 SSE 数据畸形——那是 provider bug,不应重试)。
2. `RetryPolicy::recommended()`(或等价构造):429/5xx/timeout/流中断,次数与退避给合理默认;rustdoc 写明。
3. builder/绑定透出。
4. 测试:流中断有 policy 重试成功、无 policy 维持 RunFailed、次数上限、429/5xx/timeout 不回归。
5. 四件套 + cargo doc。
