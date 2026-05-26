# v0.9 Spec：产品成熟度

## 背景

v0.7-v0.8 完成了扩展性地基（Hook 框架）、sub-agent 语义统一（Handoff）、持久化（Session）、安全层（Guardrail + 权限模型）。runtime 的核心能力已完备。

v0.9 的目标是补齐产品级 agent 应用的最后几块拼图，为首次公开发布做准备。

## 目标

1. 用户可以在 run 过程中注入新指令，不限于 approval 单次干预
2. Provider 覆盖扩大，支持主流模型
3. 文档、示例、发布流程就绪

## 范围

### Mid-run Steering

扩展 `RunHandle` 的运行时交互能力：

```rust
impl RunHandle {
    pub fn abort(&self) { ... }                          // v0.7 已有
    pub async fn inject_message(&self, msg: Message) { ... }  // 新增
    pub async fn steer(&self, instruction: &str) { ... }      // 新增
}
```

- `inject_message`：向 run 的消息队列注入一条用户消息，模型在下一轮迭代时看到
- `steer`：注入一条系统级指令（如"停止当前方向，改为..."），不作为用户消息呈现

通过内部 channel（`mpsc::Sender`）与 run loop 通信。run loop 在每次迭代前检查 steering queue。

需要 v0.8 的 Session 持久化支持——steering 消息需要纳入 session snapshot，恢复时不丢失。

竞品参考：
- pi-agent-core steering queue + follow-up queue

### Provider 扩展

基于 hotfix 落地的 `ProviderFactory` trait，新增 provider：

| Provider | 优先级 | 说明 |
|----------|--------|------|
| Google Gemini | P0 | 主流，API 差异较大，需独立 adapter |
| Ollama / 本地模型 | P1 | 本地部署场景，OpenAI 兼容 API |
| Mistral | P2 | OpenAI 兼容，复用 `openai_compat` |

每个新 provider 只需实现 `ProviderFactory` trait，不改 core。

### 文档与示例

- API 参考文档（rustdoc 级别）
- 入门教程（从零到一个可运行的 agent）
- 进阶示例：自定义 Hook、Guardrail、SessionStore、Handoff 编排
- SDK 文档（Python / TypeScript 使用指南）

### 发布准备

- crates.io 发包流程
- 版本号策略（semver）
- CHANGELOG
- CI 增加发布 pipeline

## 不在范围内

- 分布式 agent 编排（多进程 / 多机）
- 内置 observability 平台（保持 event stream + tracing 信号暴露，不内置 collector）
- Skill marketplace
- Web UI / Dashboard

## 依赖

- v0.8 Session 持久化（steering 消息需要持久化）
- v0.8 权限模型（provider 扩展后的安全模型需要权限框架支撑）

## 验收标准

- [ ] `RunHandle::inject_message()` 和 `steer()` 可用，run loop 在下一轮迭代时响应
- [ ] Steering 消息纳入 session snapshot
- [ ] Google Gemini adapter 通过集成测试
- [ ] Ollama adapter 通过本地模型集成测试
- [ ] `examples/` 覆盖 hook、guardrail、session、handoff、steering、多 provider 场景
- [ ] rustdoc 生成完整 API 文档
- [ ] crates.io 发包流程可用
- [ ] `cargo test --workspace` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
