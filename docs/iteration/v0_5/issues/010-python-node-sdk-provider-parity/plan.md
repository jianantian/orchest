# 010 实现路线

## 步骤

1. **确认 Rust contract**
   - 读取 Issue 009 落地后的 `ProviderRuntimeConfig` / config-first factory / `RequestOptions`
   - 确认 Python 和 Node.js 不需要 provider-specific adapter/config import

2. **更新 Python binding**
   - `Agent.__new__` 添加 `api_key`、`api_key_env`、`max_tokens`、`request_options`
   - 保留现有 `api_url` 和旧参数顺序兼容；新增参数放在末尾或 keyword-only
   - 将 Python dict 映射到 Rust config 和 `RequestOptions`
   - 错误信息列出允许 enum 值

3. **更新 Node binding**
   - `AgentOptions` 添加 `apiKey`、`apiKeyEnv`、`maxTokens`、`requestOptions`
   - 将 camelCase options 映射到 Rust config 和 `RequestOptions`
   - 不暴露 snake_case 字段给 JS 用户

4. **更新类型声明**
   - Python `__init__.py` / `__init__.pyi`：补 `RequestOptions`、extended `TokenUsage`、stream event hints
   - Node `js/index.d.ts` / `js/native.d.ts`：补 `RequestOptions`、`TokenUsage`、`StreamEvent`
   - 确保 generated/native declarations 和 hand-written declarations 不冲突

5. **更新 examples**
   - Python DeepSeek + thinking options
   - Python legacy Anthropic shorthand
   - Node OpenRouter nested model + requestOptions
   - Node legacy Anthropic shorthand

6. **测试与验证**
   - Python/Node 分别测试 config mapping、legacy shorthand、request options mapping
   - TypeScript type check
   - `cargo test --workspace`
   - `cargo clippy --workspace -- -D warnings`

## 要读的现有代码

- `crates/agent-runtime-py/src/lib.rs`
- `crates/agent-runtime-node/src/lib.rs`
- `python/agent_runtime/__init__.py`
- `python/agent_runtime/__init__.pyi`
- `js/index.d.ts`
- `js/native.d.ts`
- `js/index.ts`
- `examples/python_basic.py`
- `examples/ts_basic.ts`

## 关键决策

- Python uses snake_case; Node.js uses camelCase.
- Rust owns provider routing and defaulting.
- Bindings own only language conversion and type declarations.
