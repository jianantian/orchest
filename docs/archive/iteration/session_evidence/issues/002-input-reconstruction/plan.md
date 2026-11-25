# 002 实施计划

## 阅读

- [001 规格](../001-attempt-record/spec.md)
- [原设计的存储、隐私和兼容约束](../../../../../todo/session-evidence-dsh.md#存储隐私和兼容)
- `crates/orchest/src/run/actor.rs`、模型消息、工具定义和 options 的协议类型

## 预计修改

`crates/orchest/src/session/` 的证据采集策略、存储与读取，`crates/orchest/src/run/actor.rs` 的调用边界，现有 mock adapter 测试。

## 步骤

1. 实现默认字段白名单和宿主授权的内容策略。在队列入口前限制提取、复制和序列化开销。读取及写入完成时校验权限与过期状态，安排持久数据和索引清理。
2. 从实际传入的参数捕获 profile 与有序上下文，带覆盖信息写入 001 存储。引用绑定会话实例及权限范围。仅在这些条件和内容均一致时复用资料；是否做物理去重留给实现。
3. 建立重开存储后的 mock 实收对照测试，覆盖两次 hook 注入、工具改序和 options 差异。用合成数据验证超限中止、受限字段、积压期间撤销和过期。
