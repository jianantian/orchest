# Create Squad 遗珠：值得保留的设计思想

> 2026-06-01 | 从 90-130K 行的 Python+NestJS+React 代码中提取 40+ 个好点子。代码写得草，但不少想法是对的。

---

## 一、Meeting / ASR：最完整的差异化能力

### 1.1 PCM 流式 ASR + 自动恢复

```
浏览器麦克风 → PCM 二进制流 → WebSocket → backend
  → Volcengine SAUC 协议 → 实时转写
  → flush timer（静默超时后提交） + auto-recovery（断线自动重连）
```

**为什么好**：不是简单的「上传音频文件然后转写」——是实时流式转写，agent 可以在会议进行中就参与。断线自动恢复机制是生产级思想。

**Multivac 怎么做**：ASR Gateway 作为独立卫星 crate（`agent-runtime-asr-providers`），对接 Orchest 研究文档中的供应商矩阵。stream 模式通过 `AudioStream` → `TranscribeStream` pipeline。会议生命周期在 multivac-core 的 `meeting/` 模块管理。

### 1.2 会议摘要由 Avatar Employee 生成

会议结束后不是调一个通用 LLM 摘要——而是调**用户自己的 avatar employee**（同一个 model catalog、同一个 persona profile）。摘要质量受 employee 的 prompt engineering 约束，保持一致性。

**Multivac 怎么做**：Orchest agent 通过 Hook 注入 meeting transcript + voice_meta，自动决定输出策略（口语摘要 + 屏幕详细内容）。employee persona 通过 `AgentConfig.system_prompt` 表达。

### 1.3 Meeting Session 独立于 Channel

Meeting 有自己的生命周期（create → feed audio → stop → summarize），不耦合 channel chat。Meeting 结束后产出 transcript + summary 作为知识库文档。

**Multivac 怎么做**：保持这个分离。`meeting/` 模块的 `MeetingSession` 是独立实体，transcript 写入 `knowledge_docs` 表。和 session（agent conversation）不互相侵入。

---

## 二、Task 状态机：三层分离

### 2.1 Multi-layer Task State

```
TaskStatus {
  lifecycle:         pending | running | paused | completed | failed | cancelled
  runtime_projection: idle | working | waiting_for_user | processing_tools | ...
  message_dispatch:   dispatched | acknowledged | routed | ...
}
```

三层独立状态——lifecycle 是业务状态，runtime_projection 是执行引擎视角，message_dispatch 是消息路由视角。不是把所有状态塞进一个 enum。

**为什么好**：分离关注点。UI 上的「任务进行中」可能是 lifecycle=running + runtime_projection=waiting_for_user。不会混淆「agent 在等用户回复」和「agent 在跑工具」。

**Multivac 怎么做**：`Task` 模型的 `lifecycle` 字段保留。`runtime_projection` 通过 RuntimeBackend 的 `task_status()` 返回——不存储在 Task 表。`message_dispatch` 是 WebSocket 层的事，不进入 Task 模型。

### 2.2 Task Post-Processor Chain

```
Task 进入终态 → Chain of Responsibility handlers:
  1. General post-processor（通知、日志）
  2. Scheduled task handler（如果是定时任务触发的，更新调度状态）
  3. Merge queue handler（如果 task 有 pending MR，唤醒 merge queue）
```

**Multivac 怎么做**：Orchest Hook 的 `on_run_end` 是做这个的天然位置——注册一个 `TaskPostProcessHook`，检查 task 是否终态、是否关联 MR、是否是定时任务。

### 2.3 Workspace + Draft + Merge Request

每个 task 有独立 workspace（bare git repo 分支），task 执行结果作为 draft，用户可以 review 后 merge。Merge queue 是 FIFO + retry + recovery。

**为什么好**：这不是 agent 概念，是**协作**概念。Agent 产出的不是最终结果——是 draft，需要人类确认。和 GitHub PR 的 mental model 一致。

**Multivac 怎么做**：保留核心流程。但不用 git bare repo——初版用文件快照对比（`diff(old_tree, new_tree)`）。task 执行时 workspace 是工作目录的子目录，task 完成后产出 diff → draft → merge。Merge 就是 `rsync diff`。

---

## 三、Employee / Avatar：Agent 人格化

### 3.1 Employee 的三元定义

```
Employee {
  identity:   name, avatar_url, description
  soul:       system_prompt, personality_traits, expertise
  role:       skills[], tools[], model_id, scope(token_quota, project_access)
}
```

identity 是用户看到的；soul 是 agent 的行为；role 是 agent 的能力边界。不是 `AgentConfig` 的扁平字段——是有层次的人格定义。

**Multivac 怎么做**：Orchest 的 `AgentConfig` + `system_prompt` + `Skill` 三元组天然映射。Employee 的 CRUD 在 `org/` 模块，最终转为 `AgentConfig`。不需要独立的 `employee/` 模块——员工管理就是 org 管理的一部分。

### 3.2 Avatar Governance

系统预置的 avatar employee 有受保护的字段（不能被用户随意修改的核心 skill、最小 tool 集合）。用户创建的 employee 可以自由配置。

**Multivac 怎么做**：Skill 的 `SKILL.md` frontmatter 声明 `required: true`，不可被删除。Tool 通过 `Approval::Always` 保护敏感操作，不暴露给用户随意配置。

---

## 四、Channel Turn Orchestrator

### 4.1 Pre-allocate MessageId

```
用户发送消息 → 立即分配 messageId → 返回给前端
  → SSE stream 开始推送事件（text delta, tool call, thinking）
  → 最终 commit（确认消息完成）
```

不是「agent 跑完再告诉前端结果」——前端在 agent 开始跑之前就知道 messageId，可以立即渲染占位 UI。这个模式比 Nimbalyst 和 Craft Agents 的「等 SDK 返回第一个 event 才知道有东西」更流畅。

**Multivac 怎么做**：WebSocket 协议里 `user_message` 的 ack 立即返回 `message_id`。前端收到后创建 TurnCard skeleton（pending 状态），后续 events 逐步填充。和 Craft Agents 的 `TurnPhase: pending` 完全一致。

### 4.2 Multi-part Commit

```
agent 产出 → [text_part_1, tool_use, text_part_2, tool_result, text_part_3]
  → 每个 part 独立 SSE event
  → 最后一次性 commit（所有 parts 组装为一条 message 写入 DB）
```

不是每个 text delta 写一次数据库——整个 turn 的 parts 在内存中缓冲，turn 完成后一次性写入。

**Multivac 怎么做**：Orchest 的 `RuntimeEvent` 流天然是 part-by-part。`AuditHook` 在 `on_run_end` 才写入 transcript——不是每个 event 写一次 DB。

---

## 五、知识库：Locator 模式 + 跨域搜索

### 5.1 Knowledge Locator

```
knowledge://workspace/docs/design.md       → 文件型知识
knowledge://task/SYSTEM_TASK_001/summary   → task session 产出
```

不是只有「上传的文件」是知识。Agent 执行 task 的 summary 也是知识——同一套 locator 模式统一引用。

**Multivac 怎么做**：`knowledge_docs` 表不只存文件。`source: File(path) | TaskOutput(task_id) | MeetingTranscript(meeting_id)` enum。搜索索引覆盖所有来源。

### 5.2 多 Scope

```
knowledge doc scope:
  user:{user_id}     → 个人知识
  org:{org_id}       → 组织知识
  project:{proj_id}  → 项目知识
```

**Multivac 怎么做**：保留。`knowledge_docs.org_id` 和 `knowledge_docs.project_id` nullable——个人文档两字段均为 null。

---

## 六、CLI Runtime (PTY)：Rust 重写版

### 6.1 PTY Daemon + Session 持久化

```
Python agent-engine:
  PTY daemon（本地 + Docker 两种后端）
  → 会话持久化（断线后重连到同一个 PTY session）
  → replay buffer（重连后回放断线期间的输出）
  → sideband input（agent 通过独立 channel 向 PTY 注入命令，不干扰用户键盘）
```

**为什么好**：PTY 不是一次性 shell 执行——是持久化会话。Agent 和用户共享同一个终端窗口。断线后重连不会丢失上下文。Sideband input 让 agent 可以在用户不知情的情况下执行命令。

**Multivac 怎么做**：`PtyBackend` trait。初版 local 后端（`portable-pty`），session 持久化到 SQLite。replay buffer 是 `Vec<OutputChunk>` 的 ring buffer。Sideband input 通过独立 write channel。和 RuntimeBackend 的 `PtyRuntime` 不同——PtyRuntime 是 agent 的终端（Claude Code 自己管理），而 PtyBackend 是用户的 shell（用户在 UI 中看到的终端窗口）。

### 6.2 Docker PTY 后端

Agent 可以在 Docker 容器里执行，不在用户本机——安全的代码执行沙箱。

**Multivac 怎么做**：`PtyBackend` 的 `Docker` variant。不在 v0，但接口预留。

---

## 七、Tool Profiles / Groups

### 7.1 Declarative Tool Profiles

```
profile "code-review": [read_file, search_code, git_diff, edit_file]
profile "ops": [kubectl, docker_ps, aws_cli, read_logs]
```

不是每个 employee 手动勾选 tool——用预定义 profile 分组授权。

**Multivac 怎么做**：Orchest 的 Skill 天然支持这个——Skill 可以声明依赖的 tool 集合。`code-review/SKILL.md` 声明 `tools: [read_file, edit_file, git_diff]`，employee 配置时选择 skill 而非逐 tool 选择。

### 7.2 Tool Search Tool 的影子实现

create_squad 有一个 `search_tools` tool——agent 可以在运行时动态发现可用工具。这是 Orchest `ToolSearchTool`（v0.2）的前身，但实现更粗糙。

**Multivac 怎么做**：直接用 Orchest 的 `ToolSearchTool`——trigram/Jaccard 近似匹配，不需要自己实现。

---

## 八、不应保留的东西

| 想法 | 为什么不值得保留 |
|------|----------------|
| Employee 的 JSON string 数组存 skillIds/toolIds | 不可查询，不可索引——用 relation table |
| Channel 耦合 chat + task + meeting | 三个独立的实体类型混在一个模型里 |
| gRPC agent engine 独立进程 | Orchest 是 in-process library |
| Grace-call（预算耗尽时注入 system message） | Orchest BudgetGuard 应该在预算耗尽前介入 |
| ReAct loop 在 Python | Orchest AgentRun 在 Rust |
| 前端 10 个 Zustand stores | Jotai atom family |
| WebSocket + SSE 双重推送 | 只要 WebSocket（Orchest RuntimeEvent → WS） |
| 手动 trace/audit | Orchest Hook 的 AuditHook |
| model catalog 独立维护 | ModelAdapter 由 Orchest 管理 |

---

## 九、保留清单

| # | 想法 | Multivac 模块 | 优先级 |
|---|------|-------------|--------|
| 1 | PCM 流式 ASR + auto-recovery | meeting/ + ASR Gateway | 阶段 4 |
| 2 | Meeting summary by employee persona | meeting/ | 阶段 4 |
| 3 | Meeting 独立于 Session | meeting/ | 阶段 4 |
| 4 | 三层 Task state（lifecycle/runtime/message） | task/ | 阶段 0 |
| 5 | Task Post-Processor Chain | Hook `on_run_end` | 阶段 1 |
| 6 | Workspace + Draft + Merge Request | task/ | 阶段 4 |
| 7 | Employee 三元定义（identity/soul/role） | org/ → AgentConfig | 阶段 0 |
| 8 | Avatar governance（受保护字段） | Skill `required: true` | 阶段 1 |
| 9 | Pre-allocate MessageId | WebSocket 协议 | 阶段 0 |
| 10 | Multi-part commit（turn 结束统一写 DB） | AuditHook | 阶段 1 |
| 11 | Knowledge Locator（统一引用文件+task+meeting） | knowledge/ | 阶段 0 |
| 12 | 多 Scope（user/org/project） | knowledge/ | 阶段 0 |
| 13 | PTY session 持久化 + replay buffer | pty/ | 阶段 4 |
| 14 | PTY sideband input | pty/ | 阶段 4 |
| 15 | Docker PTY 后端 | pty/ Docker variant | 远期 |
| 16 | Tool Profiles → Skill 依赖 | skills/ | 阶段 1 |
