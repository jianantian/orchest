# Sub-agent 设计：Handoff vs Agent as Tool

> 日期: 2026-05-25
> 对比对象: Orchest `HEAD` / OpenAI Agents SDK `HEAD`
> 相关: `docs/research/orchest-vs-openai-claude-sdk.md`

## 1. OpenAI SDK 的二分法

OpenAI Agents SDK 把 sub-agent 清晰地分为两种语义：

```
agent.as_tool()                    Handoff
───────────────────────────        ───────────────────────────
"这个子任务交给你，结果给我"         "这个用户交给你，我不回来了"
父 agent 拿到结果继续对话           父 agent 退出，子 agent 接管会话
工具级委托                          会话级路由
```

### 1.1 Agent as Tool —— `agent.as_tool()`

**用途**：委托一个独立子任务，拿到结果后父 agent 继续。

**运行时**（`agent.py:599-660`）：

```python
async def _run_agent_impl(context, input_json):
    # 1. 解析结构化输入（Pydantic model → JSON Schema → prompt）
    parsed_params = params_adapter.validate_python(json_data)
    resolved_input = await resolve_agent_tool_input(
        params=parsed_params,
        schema_info=schema_info,      # 可选结构描述
        input_builder=input_builder,  # 可选自定义 prompt 构建器
    )

    # 2. 同步调用 Runner.run() —— 同一个 event loop
    run_result = await Runner.run(
        starting_agent=self,
        input=resolved_input,
        max_turns=max_turns,
        ...
    )

    # 3. 提取输出
    return custom_output_extractor(run_result) if custom_output_extractor \
           else run_result.final_output
```

**关键特征**：
- 同进程、同 event loop、**同步** `Runner.run()` 调用
- 模型看到的是普通 `FunctionTool`，不知道里面跑了一个 agent
- 输入由结构化 schema 自动转换为 prompt
- 输出由可选的 `custom_output_extractor` 提取
- 子 agent 审批状态通过 `agent_tool_state` 缓存管理与父 agent 双向同步

### 1.2 Handoff —— `handoff(agent)`

**用途**：将整个会话路由到另一个 agent，当前 agent 不再参与。

**运行时**（`run_loop.py:803-815`）：

```python
if isinstance(turn_result.next_step, NextStepHandoff):
    current_agent = turn_result.next_step.new_agent  # ← 只是变量赋值
    run_state._current_agent = current_agent
    current_span.finish(reset_current=True)

    # 通知消费者：agent 切换了
    streamed_result._event_queue.put_nowait(
        AgentUpdatedStreamEvent(new_agent=current_agent)
    )
    continue  # ← 回到 while 循环顶部，下一 turn 用新 agent
```

**关键特征**：
- 不 spawn、不 fork、不 IPC——**只是换了 `current_agent` 变量**
- 模型通过 `transfer_to_X` 工具**主动选择**目标 agent（声明式）
- 上下文传递：`HandoffInputData` 三层结构 + `input_filter` + `nest_handoff_history`
- 事件流发出 `AgentUpdatedStreamEvent`，消费者可据此更新 UI
- 多 handoff 检测：同时调用多个 handoff → 只执行第一个，其余报错

### 1.3 Handoff 的上下文传递机制

```
HandoffInputData:
  input_history       ← Runner.run() 调用时的原始输入（str | list[ResponseInputItem]）
  pre_handoff_items   ← 本轮之前所有 RunItem
  new_items           ← 本轮新 RunItem（含触发 handoff 的 tool_call）
  input_items         ← input_filter 过滤后的 item（可选，不设置则用 new_items）
```

**默认行为**：新 agent 看到完整对话历史。

**`input_filter`**：裁减传递给新 agent 的上下文，同时保持 session history 完整。

```python
def billing_input_filter(data: HandoffInputData) -> HandoffInputData:
    recent = data.new_items[-3:]  # 只保留最近 3 条
    return data.clone(new_items=tuple(recent))
```

**`nest_handoff_history`**：将上游历史折叠为一条嵌套消息，放入 `<CONVERSATION HISTORY>...</CONVERSATION HISTORY>` 标签中，新 agent 看到的是一条 assistant message 而非完整多 turn 历史。

## 2. Orchest 的现状

### 2.1 两个并行路径，语义重叠

```
AgentDelegate (Rust trait)           __sub_agent_request (JSON 魔法字段)
────────────────────────────         ────────────────────────────────────
实现：execute_agent_delegate()       实现：execute_sub_agent_request()
触发：ToolOutput::AgentDelegate      触发：ToolOutput::Immediate → 检查 Value 中的 __sub_agent_request 字段
执行：tokio::spawn 子 run            执行：tokio::spawn 子 run
预算：从 AgentDelegate 中提取        预算：从 JSON config["budget"] 中解析
审批：通过 active_children HashMap   审批：通过 active_children HashMap
输出：ToolOutput::Structured         输出：ToolOutput::Immediate (Value)
```

**问题**：
1. 两个路径做的事几乎一样（spawn 子 run → 转发事件 → 继承预算 → 路由审批），但各自维护一套逻辑
2. 没有一个做到 Handoff 的语义——两个都是 "call agent, get result"
3. `__sub_agent_request` 是魔法字段，不是类型安全的 trait
4. v0.6 issue 004 识别了这个问题，但只计划**合并两个路径为一个**，未区分两种语义

### 2.2 当前 run loop 中的 sub-agent 处理

```rust
// run.rs — tool dispatch 中的 sub-agent 分叉
match result {
    Ok(ToolOutput::Immediate(value)) => {
        // 魔法字段检测
        if value.get("__sub_agent_request").and_then(Value::as_bool) == Some(true) {
            value = execute_sub_agent_request(...).await;
        }
        tool_results.push(value);
    }
    Ok(ToolOutput::AgentDelegate(delegate)) => {
        let (model_output, details) = execute_agent_delegate(...).await;
        tool_results.push(model_output);
    }
    // ...
}
```

两者都在 tool dispatch 层面处理，结果都作为 `ToolResult` 返回。**没有 run loop 层面的控制流切换**。

## 3. 建议：Orchest 的 Sub-agent 重构

### 3.1 目标架构

对照 OpenAI SDK 的模式，Orchest 应该有两层：

```
┌─────────────────────────────────────────────────────┐
│                  Run Loop 层                         │
│                                                     │
│  loop {                                             │
│      model.complete() → response                    │
│      for tool_call in tool_uses {                   │
│          match tool.execute() {                     │
│              ToolOutput::Immediate(v) → push         │
│              ToolOutput::Handoff(h)   → break + 切换 │  ← 新增：控制流切换
│          }                                          │
│      }                                              │
│  }                                                  │
│                                                     │
│  ┌─────────────────────────────────────────────┐    │
│  │            Tool 层                            │    │
│  │                                             │    │
│  │  Agent.as_tool() → FunctionTool              │    │
│  │  内部同步调用 AgentRun::run_to_completion()    │    │
│  │  输入：结构化 schema → prompt                  │    │
│  │  输出：custom_output_extractor                │    │
│  └─────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────┘
```

### 3.2 Agent as Tool（轻量级）

对标 OpenAI `agent.as_tool()`，替代当前的 `AgentTool` + `AgentDelegate`。

```rust
// agent-runtime-core 新增
impl AgentConfig {
    /// 将此 agent 配置转换为 FunctionTool
    /// 模型看到普通工具，不知道里面跑了 agent
    pub fn as_tool(&self) -> Arc<dyn Tool> {
        let agent_config = self.clone();
        Arc::new(AgentAsTool {
            config: agent_config,
            input_schema: None,       // 默认：{ "input": "string" }
            output_extractor: None,   // 默认：取 final_output
        })
    }

    /// 带结构化输入
    pub fn as_tool_with_schema<S: JsonSchema>(
        &self,
        input_schema: S,
        output_extractor: Option<fn(RunResult) -> Value>,
    ) -> Arc<dyn Tool> { ... }
}

#[async_trait]
impl Tool for AgentAsTool {
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        // 同步执行子 run，不 spawn
        let (handle, mut rx) = AgentRun::start(
            self.config.clone(),
            build_prompt(input, &self.input_schema),
            ctx.model.clone(),
            ctx.registry.filtered(),
        );

        let mut output = Value::Null;
        while let Some(event) = rx.recv().await {
            // 向前转发事件作为 ChildRunEvent
            ctx.emit(ChildRunEvent { ... }).await;
            if let RunCompleted { output: o } = event { output = o; }
            if let RunFailed { error } = event { return Err(...); }
        }

        // 提取输出
        let result = self.output_extractor
            .map(|f| f(output))
            .unwrap_or(output);
        Ok(ToolOutput::Immediate(result))
    }
}
```

关键设计决策：
- **不 spawn tokio task**：与 OpenAI 一样，`AgentRun::start()` 在当前 task 中执行子 run。这样可以避免审批路由的复杂性。
- **向前转发事件**：子 run 的事件通过 `ChildRunEvent` 包装向上传播
- **结构化输入**：`input_schema` 提供 JSON Schema，自动构建 prompt（`"You are being called as a tool. Here is the input: ```json {...}```"`）

### 3.3 Handoff（重量级）

对标 OpenAI `Handoff`，是 run loop 层面的控制流切换。

```rust
/// Handoff 定义 —— 模型可见的声明式路由
pub struct Handoff {
    pub tool_name: String,
    pub tool_description: String,
    pub input_schema: Value,        // JSON Schema（给模型的工具参数定义）
    pub target: HandoffTarget,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    pub nest_history: bool,
}

pub enum HandoffTarget {
    /// 静态目标：编译时确定的 agent config
    Static(AgentConfig),
    /// 动态目标：运行时根据模型输入决定
    Dynamic(Arc<dyn Fn(Value) -> AgentConfig + Send + Sync>),
}

#[async_trait]
pub trait HandoffInputFilter: Send + Sync {
    async fn filter(&self, data: HandoffInputData) -> HandoffInputData;
}

pub struct HandoffInputData {
    pub input_history: Vec<Message>,
    pub pre_handoff_items: Vec<ContentBlock>,
    pub new_items: Vec<ContentBlock>,
}
```

**Handoff 在 Tool trait 中的表示**：

```rust
pub enum ToolOutput {
    Immediate(Value),
    Structured { model_output: Value, details: Value },
    AsyncJob(JobHandle),
    Handoff(HandoffResult),  // ← 新增
}

pub struct HandoffResult {
    pub target_agent: AgentConfig,
    pub transfer_message: String,  // 发给模型："Transferred to BillingAgent"
    pub filtered_input: Vec<Message>,  // 经过 input_filter 的历史
}
```

**Run loop 中的 Handoff 处理**：

```rust
// run.rs — 新的 handoff 分支
loop {
    // ... model call, tool dispatch ...

    for tool_call in &tool_uses {
        let result = tool.execute(tool_call.input.clone(), &ctx).await;
        match result {
            // ... Immediate, Structured, AsyncJob ...

            Ok(ToolOutput::Handoff(handoff)) => {
                // 1. 记录 handoff tool result（让当前 run 的 transcript 完整）
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: json!({"result": handoff.transfer_message}),
                });

                // 2. 发出 AgentUpdated 事件（通知消费者 agent 切换了）
                emit(&tx, RuntimeEvent::AgentUpdated {
                    previous_agent: config.name.clone(),
                    new_agent: handoff.target_agent.name.clone(),
                }).await;

                // 3. 切换 config 和 registry
                config = handoff.target_agent;
                registry = build_registry(&config).await?;
                messages = handoff.filtered_input;

                // 4. 跳过剩余 tool calls，重新进入 loop（下一 turn 用新 agent）
                break 'tool_loop;
            }
            Err(e) => { /* ... */ }
        }
    }
    // ... push tool_results, step += 1, continue loop ...
}
```

### 3.4 两种机制的使用场景对比

```rust
// 场景 1：Agent as Tool —— 代码审查子任务
let reviewer = AgentConfig::new("code-reviewer")
    .with_system_prompt("You are a code reviewer. Find bugs and suggest improvements.")
    .as_tool()
    .with_input_schema::<CodeReviewInput>()  // { file_path: string, focus_areas: string[] }
    .with_output_extractor(|result| {
        // 提取审查结果中的建议列表
        result["suggestions"].clone()
    });

// 场景 2：Handoff —— 客服路由
let billing_handoff = Handoff::static_route(
    tool_name: "transfer_to_billing",
    tool_description: "Transfer to billing agent for invoice and payment issues",
    target: billing_agent_config,
)
.with_input_filter(|data| {
    // 只保留最近 5 条消息
    data.new_items = data.new_items.into_iter().rev().take(5).rev().collect();
    data
})
.with_nest_history(true);

let triage_agent = AgentConfig::new("triage")
    .with_tool(reviewer)
    .with_handoff(billing_handoff)
    .with_handoff(support_handoff);
```

## 4. 对现有代码的影响

### 4.1 可以删除的

| 文件/符号 | 原因 |
|-----------|------|
| `tool/agent.rs` — `AgentTool` 结构体 | 被 `AgentConfig::as_tool()` 替代 |
| `run.rs` — `execute_sub_agent_request()` | 被 `ToolOutput::Handoff` 路径替代 |
| `run.rs` — `execute_agent_delegate()` | 被 `AgentAsTool::execute()` 替代 |
| `run.rs` — `__sub_agent_request` 魔法字段检测 | 不再需要 |
| `tool/mod.rs` — `AgentDelegate` 结构体 | 不再需要 |
| `tool/mod.rs` — `ToolOutput::AgentDelegate` 变体 | 不再需要 |
| `run.rs` — `active_children: HashMap<RunId, ApprovalSlot>` | Handoff 不 spawn 子 task |

### 4.2 需要新增的

| 位置 | 内容 |
|------|------|
| `run.rs` | `ToolOutput::Handoff` 分支处理（控制流切换） |
| `tool/agent_as_tool.rs` | `AgentAsTool` 实现 |
| `handoff.rs` | `Handoff`、`HandoffTarget`、`HandoffInputFilter`、`HandoffInputData` |
| `events.rs` | `RuntimeEvent::AgentUpdated` |
| `agent_config.rs` | `AgentConfig::as_tool()` + builder 方法 |

### 4.3 需要修改的

| 位置 | 变更 |
|------|------|
| `tool/mod.rs` | 删除 `AgentDelegate`，新增 `HandoffResult` |
| `run.rs` | 删除 `execute_sub_agent_request` 和 `execute_agent_delegate`，新增 handoff 分支 |
| `run/handle.rs` (v0.6 001) | 简化 RunHandle —— 不再需要 `active_children` |
| `budget.rs` | Handoff 时 budget 处理：是继承剩余预算还是独立预算？ |

## 5. 实施路线

### v0.6（已规划，不改语义）
- 004 合并 `__sub_agent_request` 和 `AgentDelegate` 为单一路径（最小改动，不改架构）

### v0.7（建议）
1. **引入 `ToolOutput::Handoff`**：在 run loop 中新增 control-flow 切换分支
2. **实现 `AgentConfig::as_tool()`**：替代 `AgentTool`，作为同步子 run 工具
3. **删除旧路径**：移除 `AgentDelegate`、`__sub_agent_request`、`active_children`
4. **新增 `AgentUpdated` 事件**：通知 SDK 消费者 agent 切换

### 设计原则

1. **Agent as Tool 是工具**——它在 tool dispatch 层面处理，结果作为 tool result 返回
2. **Handoff 是控制流**——它在 run loop 层面处理，切换 agent 后重新进入 loop
3. **不做进程隔离**——两者都是同进程内执行（与 OpenAI 一致）。进程隔离是 sandbox/deployment 层的事，不是 agent 框架层的事
4. **类型安全**——输入 schema 用 Rust 类型系统表达，不依赖魔法 JSON 字段
