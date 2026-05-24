# 003 · Tool Search Tool（渐进式 Tool 加载）

## 背景

当 tool 数量很大时（大量 MCP server 接入后），把所有 tool schema 放进 context 会大量消耗 token 预算。Tool Search Tool 允许 agent 按需检索相关 tool，只把需要的 tool schema 加载进 context。

## 目标

实现内置 `search_tools` tool，基于文本匹配检索相关 tool，并实现"隐藏 tool schema，按需暴露"的工作模式。

## 验收标准

**search_tools tool：**
- [ ] `SearchToolsTool` 实现 `Tool` trait，`source` 为 `ToolSource::Builtin`
- [ ] input schema：`{ "query": { "type": "string" }, "top_k": { "type": "integer", "default": 5 } }`
- [ ] 返回 top-K 相关 tool 的完整 schema（JSON array）
- [ ] 匹配算法：对 `query` 和每个 tool 的 `name + description` 做 trigram 匹配，返回得分最高的 K 个
- [ ] 不引入 embedding 或外部 ML 依赖

**隐藏模式（tool_search_enabled: true）：**
- [ ] `AgentConfig.tool_search_enabled: bool`（默认 false）
- [ ] 启用时，model 调用只传入 `search_tools` tool 的 schema，其余 tool schema 不传入
- [ ] 当 agent 调用 `search_tools` 并得到结果后，runtime 把返回的 tool schema 追加进 tools 列表
- [ ] 一次 run 中 tool schema 只增加不减少（已加载的 tool 不会被移除）

**未启用时行为不变：**
- [ ] `tool_search_enabled: false` 时，`search_tools` 不注册，所有 tool schema 正常传入模型

## 说明

Trigram 匹配实现参考：把字符串切成长度为 3 的 ngram 集合，计算两个集合的 Jaccard 相似度。实现约 50 行 Rust，不需要外部 crate。
