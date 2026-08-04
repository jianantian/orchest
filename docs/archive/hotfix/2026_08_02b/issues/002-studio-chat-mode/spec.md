# 002 — 后端:/api/chat studio 协作模式

## 背景

创作室的 AI 协作需要**多轮对话式**修改:用户说"把副歌改得更克制"或"风格换成 City Pop",AI 对话回应并只回传被改的字段。现有基础设施几乎全部现成:

- 标记协议 `<<<LYRICS>>>`/`<<<STYLE>>>`/`<<<TITLE>>>`/`<<<VOCAL>>>` 的解析与 `Done` 事件回传(`agent.rs:27` `parse_lyrics`)可直接复用;
- 但 `chat_handler` 在文本含 `<<<LYRICS>>>` 时必跑 `finalize_chat_output`(elevate/review 质量管道,`routes.rs:134-136`)——Studio 里每轮定向修改都会被它重写并多花几十秒,**必须跳过**(产品决策:引导模式保留,Studio 跳过)。

## 目标/范围

在 `examples/demo/music-gift/src/` 内:

1. **`ChatRequest` 扩展**(`agent/message.rs`):新增可选字段 `mode: Option<String>` 与 `draft: Option<StudioDraft>`;`StudioDraft { lyrics, style, title, vocal }`(均 `Option<String>`,`#[serde(default)]`)。不带 `mode` 的现有客户端行为完全不变。
2. **Studio 系统提示词**(`prompts.rs` 新增 `STUDIO_SYSTEM_PROMPT`):角色是协作创作伙伴;收到当前草稿全文(经 `draft` 注入,不含 guided 的 "Known info" meta 块);约定:用户要求修改时先对话式简短确认改动点,然后**只用标记块输出被修改的字段**(未改的字段不输出标记);不输出 review 表格、不输出 `<<<READY>>>` 等 guided 协议标记。
3. **`chat_handler` 分支**(`routes.rs`):`mode == "studio"` 时
   - 用 studio 系统提示词构建系统消息(draft 注入);
   - 跳过 `finalize_chat_output`,直接 `parse_lyrics` 后构造 `Done`(照带回传解析出的 `lyrics/style/title/vocal`,模型没输出的字段自然为 `None`,前端据此只应用变更字段);
   - 无 `<<<LYRICS>>>` 的纯对话轮按现有逻辑返回(`has_lyrics: false`)。
4. skills 目录注入与 guided 保持一致(同一 `run_chat_agent` 路径)。

## 验收标准

- [ ] `mode: "studio"` + 带 draft 的请求:系统消息包含草稿全文与 studio 角色提示,不含 guided 的 SYSTEM_PROMPT
- [ ] studio 模式下输出含 `<<<LYRICS>>>` 时**不触发** elevate/review(无 `Elevating`/`Reviewing` 事件,`Done` 直接带原文解析结果)
- [ ] 模型只输出 `<<<STYLE>>>` 时,`Done` 的 `lyrics` 为 `None`、`style` 有值(`has_lyrics: false`)
- [ ] 不带 `mode` 的请求行为与现状一致(质量管道照常,现有测试不破)
- [ ] 单元测试:studio 系统消息构建、studio 跳过质量管道、部分标记解析回传

## Notes

- 前端一次性 AI 小工具(写/改/扩写/polish)在 003 中移除,改走本模式;`/api/polish-music-prompt` 端点保留不删(无破坏性),但创作室不再调用。

## 实施步骤(plan)

读:

- `src/agent/message.rs`(`ChatRequest`/`build_system_message`)
- `src/agent.rs`(`parse_lyrics`、`finalize_chat_output`、`SseEvent`)
- `src/prompts.rs`(`SYSTEM_PROMPT` 结构)
- `src/routes.rs:108-172`(`chat_handler`)

改:

1. `src/agent/message.rs`:`ChatRequest` 加 `mode`/`draft`;`StudioDraft` 定义(放 `agent.rs` 或 `message.rs`,随既有类型就近);`build_studio_system_message(draft, photo_count)`。
2. `src/prompts.rs`:`STUDIO_SYSTEM_PROMPT`。
3. `src/routes.rs` `chat_handler`:按 `mode` 分支系统消息与 finalize 跳过;`Done` 构造保持现有字段。
4. 单元测试:`message.rs`(studio 系统消息)、`agent.rs` 或 handler 层(跳过 finalize 的分支)。
5. `cargo test -p <demo 包名>` + `cargo clippy -- -D warnings` + `cargo fmt --check`。
