# 005 · Context Compaction

## 背景

超长 agent run（大量工具调用、长文档处理）会导致消息历史不断增长，最终超出模型 context window。Context compaction 通过自动摘要历史消息，让 session 得以持续运行。

## 目标

实现自动 context compaction：当 context 使用比例超过阈值时，压缩早期历史，保持 session 不中断。

## 验收标准

**触发条件：**
- [ ] `AgentConfig` 新增 `compaction_threshold: Option<f32>`（0.0–1.0，默认 None 表示不启用）；该字段是 context 管理策略，不属于资源预算，不放在 `BudgetConfig`
- [ ] 每次模型调用后检查：`used_tokens / context_window_size >= compaction_threshold` 时触发

**压缩策略：**
- [ ] 保留：system prompt + 最近 N 轮对话（N 默认 10，可配置）
- [ ] 压缩：其余历史消息通过一次模型调用生成摘要
- [ ] 摘要调用不记入 `step` 计数，不发出 `ModelCallStarted`/`ModelCallCompleted` 事件
- [ ] 压缩后的 `messages` 格式：`[system_prompt_msg, summary_msg, recent_N_turns...]`

**事件：**
- [ ] 压缩完成后发出 `ContextCompacted { removed_messages: usize, summary_tokens: u32 }`
- [ ] `RuntimeEvent` enum 新增该变体

**保守性：**
- [ ] 压缩失败时（摘要调用出错）记录 warning，保留原始消息继续运行，不中止 run
- [ ] 连续两次 compaction 之间至少间隔 5 轮对话，防止频繁触发

## 摘要 Prompt

```
以下是一次 AI agent 任务的历史对话记录。请用简洁的中文总结这段历史中发生的关键事件：
完成了哪些工具调用、获取了哪些信息、做出了哪些决策。保留足够细节让 agent 能够继续任务。

{历史消息}
```
