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

### 决策 1:引入 `RunInput`,`AgentRun::start` 的 `input` 参数改为具体类型 `RunInput`

**实现过程中推翻了最初"泛化为 `impl Into<RunInput>`"的方案**——原因是一个用
`rustc` 实测验证过的 Rust 推断限制,记在下面,再给出改用具体类型 `RunInput`
的理由。

```rust
// crates/orchest/src/run/config.rs(与 AgentConfig 同模块,经 run/mod.rs pub use)

/// `AgentRun::start` 的单个 user turn 输入:一段文本,或文本+图片等
/// 多模态 block 的组合。只表达**一个 user turn**,不是完整消息历史。
#[derive(Debug, Clone, PartialEq)]
pub struct RunInput {
    blocks: Vec<ContentBlock>,
}

/// `RunInput::from_blocks` 收到不属于"用户输入"语义的 block 时返回。
#[derive(Debug, Clone, thiserror::Error)]
#[error("content block `{kind}` is not valid as run input (only Text/Image/Video/Audio are)")]
pub struct RunInputError {
    kind: &'static str,
}

impl RunInput {
    /// 纯文本输入(与旧 `input: String` 等价)。
    pub fn text(text: impl Into<String>) -> Self;

    /// 从任意 content block 序列构造。只接受 `Text`/`Image`/`Video`/`Audio` 四种
    /// 表达"用户可携带的输入内容"的 block;`ToolUse`/`ToolResult`/`Thinking`/
    /// `MidConvSystem` 是 agent loop 内部产出或 provider 专属语义,塞进首个
    /// user turn 是无效组合(例如凭空的 `ToolResult` 对不上任何 `tool_use_id`),
    /// 一律拒绝而非静默接受或 panic。
    pub fn from_blocks(blocks: Vec<ContentBlock>) -> Result<Self, RunInputError>;

    /// 便捷追加一张图片(常见路径:文本问题 + 一张图)。
    pub fn with_image(self, source: MediaSource) -> Self;

    pub(crate) fn into_blocks(self) -> Vec<ContentBlock>;
}

impl From<String> for RunInput          // Text block,infallible
impl From<&str> for RunInput            // Text block,infallible
```

签名变化:

```rust
// 之前
pub fn start(config: AgentConfig, input: String, ...) -> (RunHandle, EventReceiver)
// 之后(最终落地版本,不是 impl Into<RunInput>)
pub fn start(config: AgentConfig, input: RunInput, ...) -> (RunHandle, EventReceiver)
```

**为什么不是 `impl Into<RunInput>`**:全仓库现存调用点几乎都写成
`AgentRun::start(config, "hi".into(), model, registry)`——字符串字面量后面跟
`.into()`。用 `rustc` 单独验证过:当参数类型是 **泛型** `impl Into<RunInput>`
时,`"hi".into()` 无法编译(`error[E0283]: type annotations needed`)。原因是
标准库有一条反身性 blanket impl `impl<T> From<T> for T`,使 `&str:
Into<&str>` 恒成立;参数类型一旦是不确定的泛型 `T: Into<RunInput>`,编译器就
没有唯一的"期望类型"可以下推给 `.into()`,而 `&str` 同时满足
`Into<&str>`(恒成立)、`Into<String>`、`Into<RunInput>` 等多个候选,产生歧义
——这不是 `RunInput` 特有的缺陷,给 `impl Into<String>` 这种最常见的写法同样
会复现(已用同一份 rustc 验证)。只有当参数类型是**具体类型**(非泛型)时,
期望类型才会正确下推给 `.into()`,让编译器唯一确定要调用哪个 `From` 实现。

实测影响面:全仓库 `grep -c "AgentRun::start("` 命中 ~95 处,其中
`crates/orchest/src/run/tests.rs`(75 处)、`guardrail/tests.rs`(6 处)、
`examples/rust/*`(约 14 处)清一色是 `"字面量".into()` 写法。若采用泛型
`impl Into<RunInput>`,这近 90 处全部编译失败,需要逐个改成去掉 `.into()`
或改用 `.to_string()`。改用**具体类型 `RunInput`** 后,这近 90 处**零改动
编译**(参数类型固定,`.into()` 的期望类型下推正常工作);代价转移到另一侧
的少数调用点——凡是"变量已经是 `String` 类型、不写 `.into()` 直接传入"的
调用点(`orchest-py/src/lib.rs:738,803`、`orchest-node/src/lib.rs:608,665`、
`examples/demo/briefing-desk/src/app.rs` 的 `args.question.clone()`、
`examples/rust/agents/deep_research_agent.rs` 的 `input` 变量,合计 5 处)
需要补一个 `.into()` 或显式 `RunInput::text(..)`。5 处 vs. 90 处,具体类型
是净大幅减少改动量的选择。

**`from_blocks` 是 fallible**:不提供 `impl From<Vec<ContentBlock>> for
RunInput`,因为 `From` 约定是 infallible 转换,校验逻辑放进不可失败的
trait 里等于放弃校验(要么悄悄放行非法 block,要么在 `From::from` 里
panic,两者都是本次要清偿的同类"builder 该报错却没报错"问题,详见 issue
004 的 `SubAgentBuilder`)。多模态输入走独立的 `RunInput::from_blocks(vec![...])?`
构造路径,校验通过后得到的 `RunInput` 再传给 `start`——因为 `start` 现在收
具体类型 `RunInput`,这条路径不受上面的泛型歧义问题影响。

### 决策 2:内部管线 `input: String` → `input: Vec<ContentBlock>`

- `AgentRunArgs.input`(`run/actor.rs:120`)与 `start_with_bus` 的 `input` 参数改为 `Vec<ContentBlock>`(均非公开面,自由改)
- actor 组装处(`actor.rs:285-288`)从 `vec![ContentBlock::Text(input)]` 改为直接使用 blocks
- supervisor 重启克隆 `AgentRunArgs` 原样携带,无逻辑变化
- 空输入语义保持现状不变:空 `String` 仍生成空 Text block 的 user message;`RunInput::from_blocks(vec![])` 是 `Ok`,生成空 content 的 user message——空 `Vec` 不含任何非法 block,校验不拦它,行为与旧 `input: String` 对齐

### 决策 3:补 re-export

`crates/orchest/src/model/mod.rs` 的 `pub use orchest_protocol::{...}` 增加 `MediaSource`。`RunInput` 经 `run/mod.rs` 现有 `pub use config::{...}` 导出。

### 被否决的备选

| 备选 | 否决理由 |
|------|---------|
| A. 另加 `AgentRun::start_with_blocks(...)` 兄弟入口 | v1.0 即将冻结公开 API,两个并列入口意味着永久维护两份文档与两份 binding 映射;单一 `RunInput` 参数类型(`RunInput::text(..)` 覆盖纯文本、`.with_image(..)`/`from_blocks(..)` 覆盖多模态)一个入口覆盖两种用法,不需要兄弟入口 |
| B. 窄化公开 `start_with_bus` | 会把 `ApprovalBus`、`initial_messages`(内部上下文注入语义)、8 参数签名一并冻结进 v1.0 公开面;`initial_messages` 的语义(system 与 user turn 之间的前置历史)也不是"图片输入"的正确表达 |
| C. `AgentRun::start` 收 `Vec<Message>` 完整历史 | 完整历史播种是"上下文注入"特性,角色不变量(system 只能一条且在首位、user/assistant 交替性由 provider 各自约束)需要一套校验,超出本 blocker 的修复范围;内部 `initial_messages` 已服务唯一现存用例(Agent-as-Tool Fork) |
| D. `input` 参数泛化为 `impl Into<RunInput>`(最初方案) | 用 rustc 实测证伪:全仓库 ~90 处 `"字面量".into()` 调用点在泛型参数下无法编译(见上方决策 1 的推导),只有 5 处"裸 `String` 变量"调用点受益。具体类型 `RunInput` 反过来:裸变量调用点补一个 `.into()`,字面量调用点零改动,净改动量小一个数量级 |

### 非目标(明确不做)

- **`ToolResult.content: Value` 类型改造**(工具向下一轮对话注入 Image):这是横穿 `orchest-protocol` 所有 provider 序列化器的破坏性协议变更,风险与本 hotfix 体量不符。demo 的 `describe_image` 不依赖它——见下方"demo 落地方式"。若后续有真实需求,单独立 issue 走协议演进流程
- **Python/TS binding 暴露多模态入参**:binding 现有调用零改动编译即可;暴露 blocks 是 post-hotfix 工作
- **`ContentBlock::Video`/`Audio` 的入口便捷方法**:`from_blocks` 已可携带,不为尚无 demo 验证的模态加糖

## demo 落地方式(验收标准第 2 条)

`examples/demo/briefing-desk` 的 `DescribeImageTool`(`src/media.rs`,原先是文档化的固定文本占位)改为真实视觉调用:

- 工具构造时持有 `Arc<dyn ModelAdapter>`(`app.rs` 注册时传入)
- `execute()` 读取 corpus 图片文件 → base64 → `ContentBlock::Image { source: MediaSource::Base64 { media_type, data }, detail: None }`,与一条描述指令文本一起组成单条 user message,直接调 `ModelAdapter::complete()`,把返回文本作为 tool result
- **`--fake` 模式**:新增专属的 `DescribeImageFakeModel`(`fake_model.rs`,与既有 `FakeModel`/`ReviewerFakeModel` 同模式),忽略实际图片字节返回固定描述——调用路径是真的(真实 `ContentBlock::Image` 构造 + 真实 `ModelAdapter::complete()` 调用),只有返回内容是确定性的假数据
- **live 模式**:demo 当前完全没有 live 聊天模型的接线(`app.rs` 的 `run()` 在 `!args.fake` 时直接报错 "live provider mode is not implemented yet"——这是 demo 脚手架本身的既有结构性限制,不是 #195 的范围;#195 只保证"图片能到达 `ModelAdapter::complete()`"这条路径存在且类型正确,不负责给这个 demo 接一个从未有过的 live 聊天 provider)
- 另在 `examples/rust/multimodal_image_input.rs` 增加一条**入口路径**用法:`AgentRun::start(config, RunInput::text("...").with_image(source), model, registry)`,与工具内直调(`DescribeImageTool`)一起覆盖两条路径

## 测试

1. `RunInput` 单元测试:`From<String>`/`From<&str>`/`with_image` 的 block 构成
2. `from_blocks` 校验测试:`Text`/`Image`/`Video`/`Audio`(含四者混合、空 `Vec`)返回 `Ok`;`ToolUse`/`ToolResult`/`Thinking`/`MidConvSystem` 各自返回 `Err(RunInputError)` 且 `kind` 字段能定位具体 block 种类
3. actor 级测试:`start` 传入含 Image 的 `RunInput`,断言发给 `ModelAdapter::complete()` 的 messages 中 user turn 携带 `ContentBlock::Image`(fake adapter 捕获入参,现有测试基建已有此模式)
4. session 回归:含 Image block 的 run 经 `SessionStore` 存取后 blocks 无损(`ContentBlock` 本就 `Serialize`,钉住防回归)
5. 现有全部测试零破坏(内部 `"字面量".into()` 调用点靠具体类型参数保持零改动;5 处裸变量调用点补 `.into()`/`RunInput::text(..)`)

## 验收标准(对齐 GitHub #195)

- [x] `RunInput` 类型 + `AgentRun::start` 参数类型落地(具体类型 `RunInput`,非 `impl Into<RunInput>`——见决策 1 的 rustc 实测),`MediaSource` 从 `orchest::model` 可达
- [x] `from_blocks` 对 `ToolUse`/`ToolResult`/`Thinking`/`MidConvSystem` 返回 `Err(RunInputError)`,对 `Text`/`Image`/`Video`/`Audio` 返回 `Ok`,测试覆盖全部 8 种 block(`crates/orchest/src/run/config.rs` 测试模块)
- [x] `cargo check --workspace --all-targets` 过;仅 5 处"裸变量"调用点(2 py binding、2 node binding、1 demo)需要改动,其余 ~90 处内部 `"字面量".into()` 调用点零改动
- [x] `describe_image` 工具真实构造 `ContentBlock::Image` 并进入真实 `ModelAdapter::complete()` 调用(`DescribeImageFakeModel` 承接 `--fake` 路径)
- [x] `cargo test -p briefing-desk-demo`(20 个测试全绿)+ `--fake` 手动 run 重跑,行为与改动前一致(见下方输出)
- [ ] `docs/review/v0_10_demo_validation.md` Freeze Coverage Statement 的 "Multimodal image input" 行更新——留给 006(demo 重验证 + 报告收尾)统一处理,不在本 issue 单独做
- [x] `AgentRun::start` rustdoc 与 `docs/guide/quickstart.md` 补多模态输入说明

## 实现记录

- `cargo test --workspace --features orchest/sqlite-session`:全部通过,0 failed
- `cargo clippy --workspace --all-targets -- -D warnings`:无新增 finding(仓库里有两处与本 issue 无关的既有 clippy 失败,`crates/orchest-provider/tests/selection.rs` 的 `result_large_err` 与 `crates/orchest/src/run/tests.rs:2811` 的 `too_many_arguments`,在改动前的基线提交上复现过,不属于本 issue 范围)
- `cargo fmt --check`:通过
- `bash scripts/lint-check.sh`:通过(exit 0)
- `cargo run -p briefing-desk-demo -- run --materials examples/demo/briefing-desk/fixtures/research --question "..." --output <path> --fake`:手动跑通,`describe_image` 输出与改动前一致的固定描述文本,brief 正常生成
