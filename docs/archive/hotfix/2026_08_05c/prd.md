# Explicit Agent Name

`RunHookContext` 与 handoff 日志目前从 `system_prompt` 截取临时 agent 名称，混淆了身份与提示词语义。

本 hotfix 为 Rust、Python、Node 的 agent 配置增加必填 `name`，并让 run hook、handoff hook 与 `AgentUpdated` 事件直接使用该字段。旧构造签名和 prompt fallback 不保留。

