# Issue 001:多模态图片输入公开 API(`RunInput`)

GitHub: [#195](https://github.com/jianantian/orchest/issues/195) · release-blocker

## 现状(设计输入)

逐条核对过源码的事实:

1. **入口只收 `String`**:`AgentRun::start(config, input: String, model, registry)`(`crates/orchest/src/run/mod.rs:37-51`)是唯一公开入口。actor 内部把它定型为单个 Text block 的 user message(`crates/orchest/src/run/actor.rs:285-288`)。
2. **能收 `Vec<Message>` 的 `start_with_bus` 是 `pub(crate)`**(`run/mod.rs:54`),且其 `initial_messages` 参数语义是"system prompt 与 user input 之间的前置上下文"(Agent-as-Tool 的 `ContextMode::Fork` 用它,`tool/agent_as_tool.rs:132-155`;supervisor 重启也依赖它,`run/supervisor.rs:308`),不是"user turn 的多模态内容"。
3. **协议与 provider 层已就绪**:`ContentBlock::Image { source: MediaSource, detail }` 自 v0.9.10 就在 `orchest-protocol/src/types.rs:64-68`;Anthropic 与 Minimax 的请求构造都能序列化它(`orchest-provider-http/src/providers/anthropic/request.rs:227-237`、`providers/minimax/request.rs`);compaction 的 size 估算也已覆盖 Image/Video/Audio(`run/actor.rs:2220-2229`)。**缺口纯粹在 runtime 入口这一层。**
4. **re-export 缺口**:`orchest::model` re-export 了 `ContentBlock`/`Message`/`Role`(`crates/orchest/src/model/mod.rs:4-9`)但**没有 `MediaSource`**——下游即使拿到多模态入口,构造 `ContentBlock::Image` 还得直接依赖 `orchest-protocol`,违反"node/py 只经 protocol + 墙"之外的下游应经 `orchest` 门面的约定。
5. **两个 SDK binding 都以 Rust `String` 调 `AgentRun::start`**(`orchest-py/src/lib.rs:738,803`、`orchest-node/src/lib.rs:608,665`)。
6. `ContentBlock::ToolResult.content` 定型 `serde_json::Value`,工具无法向下一轮注入 Image(issue 里"另一个方向"的发现)。

## 设计决策

### 决策 1:引入 `RunInput`,`AgentRun::start` 的 `input` 参数泛化为 `impl Into<RunInput>`

```rust
// crates/orchest/src/run/config.rs(与 AgentConfig 同模块,经 run/mod.rs pub use)

/// `AgentRun::start` 的单个 user turn 输入:一段文本,或文本+图片等
/// 多模态 block 的组合。只表达**一个 user turn**,不是完整消息历史。
#[derive(Debug, Clone, PartialEq)]
pub struct RunInput {
    blocks: Vec<ContentBlock>,
}

impl RunInput {
    /// 纯文本输入(与旧 `input: String` 等价)。
    pub fn text(text: impl Into<String>) -> Self;

    /// 从任意 content block 序列构造。调用方自组 `ContentBlock::Image` 等。
    pub fn from_blocks(blocks: Vec<ContentBlock>) -> Self;

    /// 便捷追加一张图片(常见路径:文本问题 + 一张图)。
    pub fn with_image(self, source: MediaSource) -> Self;

    pub(crate) fn into_blocks(self) -> Vec<ContentBlock>;
}

impl From<String> for RunInput          // Text block
impl From<&str> for RunInput            // Text block
impl From<Vec<ContentBlock>> for RunInput
```

签名变化:

```rust
// 之前
pub fn start(config: AgentConfig, input: String, ...) -> (RunHandle, EventReceiver)
// 之后
pub fn start(config: AgentConfig, input: impl Into<RunInput>, ...) -> (RunHandle, EventReceiver)
```

**兼容性**:`From<String>` 保证所有现存调用点(两个 SDK binding、全部 examples、agent_as_tool 内部转发)**零改动编译**。这是改 `start` 本体而非另加 `start_with_blocks` 兄弟入口的前提。

### 决策 2:内部管线 `input: String` → `input: Vec<ContentBlock>`

- `AgentRunArgs.input`(`run/actor.rs:120`)与 `start_with_bus` 的 `input` 参数改为 `Vec<ContentBlock>`(均非公开面,自由改)
- actor 组装处(`actor.rs:285-288`)从 `vec![ContentBlock::Text(input)]` 改为直接使用 blocks
- supervisor 重启克隆 `AgentRunArgs` 原样携带,无逻辑变化
- 空输入语义保持现状不变:空 `String` 现在会生成空 Text block 的 user message,`RunInput::from_blocks(vec![])` 生成空 content 的 user message,不新增校验(与旧行为对齐,校验属 post-1.0 讨论)

### 决策 3:补 re-export

`crates/orchest/src/model/mod.rs` 的 `pub use orchest_protocol::{...}` 增加 `MediaSource`。`RunInput` 经 `run/mod.rs` 现有 `pub use config::{...}` 导出。

### 被否决的备选

| 备选 | 否决理由 |
|------|---------|
| A. 另加 `AgentRun::start_with_blocks(...)` 兄弟入口 | v1.0 即将冻结公开 API,两个并列入口意味着永久维护两份文档与两份 binding 映射;`impl Into<RunInput>` 一个入口覆盖两种用法且不破坏现有调用 |
| B. 窄化公开 `start_with_bus` | 会把 `ApprovalBus`、`initial_messages`(内部上下文注入语义)、8 参数签名一并冻结进 v1.0 公开面;`initial_messages` 的语义(system 与 user turn 之间的前置历史)也不是"图片输入"的正确表达 |
| C. `AgentRun::start` 收 `Vec<Message>` 完整历史 | 完整历史播种是"上下文注入"特性,角色不变量(system 只能一条且在首位、user/assistant 交替性由 provider 各自约束)需要一套校验,超出本 blocker 的修复范围;内部 `initial_messages` 已服务唯一现存用例(Agent-as-Tool Fork) |

### 非目标(明确不做)

- **`ToolResult.content: Value` 类型改造**(工具向下一轮对话注入 Image):这是横穿 `orchest-protocol` 所有 provider 序列化器的破坏性协议变更,风险与本 hotfix 体量不符。demo 的 `describe_image` 不依赖它——见下方"demo 落地方式"。若后续有真实需求,单独立 issue 走协议演进流程
- **Python/TS binding 暴露多模态入参**:binding 现有调用零改动编译即可;暴露 blocks 是 post-hotfix 工作
- **`ContentBlock::Video`/`Audio` 的入口便捷方法**:`from_blocks` 已可携带,不为尚无 demo 验证的模态加糖

## demo 落地方式(验收标准第 2 条)

`examples/demo/briefing-desk` 的 `DescribeImageTool`(`src/media.rs`,当前是文档化的固定文本占位)改为真实视觉调用:

- 工具构造时持有 `Arc<dyn ModelAdapter>`(demo 的 app 层已有 adapter 可传)
- `execute()` 读取 corpus 图片文件 → base64 → `ContentBlock::Image { source: MediaSource::Base64 { media_type, data }, detail: None }`,与一条描述指令文本一起组成单条 user message,直接调 `ModelAdapter::complete()`,把返回文本作为 tool result
- `--fake` 模式:fake adapter 返回固定描述(现有 fake 基建),live 模式走真实 Anthropic 视觉调用(demo 的 live 模型即 Anthropic,其 adapter 已支持 Image 序列化)
- 另在 demo 或 `examples/rust` 增加一条**入口路径**用法:`AgentRun::start(config, RunInput::text("描述这张图").with_image(source), ...)`,保证两条路径(user turn 携带 / 工具内直调)都有可运行示例

## 测试

1. `RunInput` 单元测试:`From<String>`/`From<&str>`/`from_blocks`/`with_image` 的 block 构成
2. actor 级测试:`start` 传入含 Image 的 `RunInput`,断言发给 `ModelAdapter::complete()` 的 messages 中 user turn 携带 `ContentBlock::Image`(fake adapter 捕获入参,现有测试基建已有此模式)
3. session 回归:含 Image block 的 run 经 `SessionStore` 存取后 blocks 无损(`ContentBlock` 本就 `Serialize`,钉住防回归)
4. 现有全部测试零破坏(签名经 `Into` 兼容)

## 验收标准(对齐 GitHub #195)

- [ ] `RunInput` 类型 + `AgentRun::start` 泛化落地,`MediaSource` 从 `orchest::model` 可达
- [ ] 现存调用点(binding、examples、内部)零改动编译;`cargo test --workspace` 过
- [ ] `describe_image` 工具真实构造 `ContentBlock::Image` 并进入真实 `ModelAdapter::complete()` 调用
- [ ] `cargo test -p briefing-desk-demo` + `--fake` 手动 run 重跑,输出贴回 #195 或关闭它的 PR
- [ ] `docs/review/v0_10_demo_validation.md` Freeze Coverage Statement 的 "Multimodal image input" 行更新
- [ ] `AgentRun::start` rustdoc 与 `docs/guide/quickstart.md` 补多模态输入说明
