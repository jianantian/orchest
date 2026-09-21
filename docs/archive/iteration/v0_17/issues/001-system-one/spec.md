# 通用 Decision 能力与首个 OpenRouter 实现

状态：已完成（2026-09-22）。GitHub [#295](https://github.com/jianantian/orchest/issues/295)（PR [#296](https://github.com/jianantian/orchest/pull/296)）。
live 验证已完成（2026-09-21，Rust/Python/TypeScript 各一次真调用全部成功，见[证据](../../../../../review/evidence/v0_17_live_decisions/README.md)）。

## Background

调用方需要一次提交共享 state 和多个独立判断，并在代码中组合其结果。
现有 ChatModel 不具有这种输入输出契约。使用独立能力可保持 minimal core 与
provider wall，同时让应用把判断用于工具、guardrail、检索或业务路由。

## Goal / scope

### 公共契约

`orchest-protocol` 新增 `Decision` trait（`async-trait`、`Send + Sync`）：

- `provider_name()`、`model_name()`、`descriptor()`。
- `decide(DecisionRequest) -> Result<DecisionResponse, ProtocolError>`。
- `Capability::Decision`，非 streaming、不提供 tool calling。
- `DecisionRequest` 含 `state` 和按 ID 索引的 `questions`；model 在构造时绑定。
- `DecisionQuestion` / `DecisionAnswer` 使用 `type` 区分 Boolean、Choice、Score。

问题 ID 是关联键，不生成、重命名或重排成匿名列表。state 支持任意 JSON；instructions 和
criteria 描述支持 JSON 字符串、对象或数组。Choice 描述另外允许 null。
Boolean criteria 可缺省，存在时含 `true` 与 `false` 两项。

响应包含 `model` 和 `answers`；`usage` 可缺省，以支持不计 token 的本地实现。
usage 包含 input_tokens/output_tokens 和可选 cost_usd；可选 id/provider 保留来源身份。

| 类型 | 必填结果 | 可选结果 | 语义 |
|------|----------|----------|------|
| Boolean | `probability: f64` | 无 | yes 的概率，范围 0–1，不自动转 bool |
| Choice | `choice: String` | `probabilities`、`confidence` | 选项必须来自该问题 criteria |
| Score | `score: f64` | `legend`、`probabilities`、`confidence` | 从 0 开始、允许小数的加权等级 |

可选值使用 Option / NotRequired / optional property；缺省不补 0 或空概率分布。
score legend 的值允许结构化描述。保留服务返回的概率与置信度，不重新估算。

### 能力边界（维护者补充要求）

- 通用能力处理共享 state + 独立 Boolean/Choice/Score 问题，结果驱动调用方代码。
- 公共 serde type 为 `boolean` / `choice` / `score`；Boolean 的 `probability` 表示 P(true)。
- `noul` 及其同名结果字段仅存在于 OpenRouter wire DTO；必须显式映射。
- state 可为任意 JSON 值；provider 对不支持的外形返回 InvalidRequest，不污染通用 trait。
- instructions/criteria 描述的通用形状为字符串、对象或数组；Choice 描述还允许 null。
- 公共 `decide` 要求显式 `provider/model`；默认模型仅属于 provider 的 registry 选择。
- 自定义 Decision 通过 Registry 注册；无 HTTP feature、凭证或 usage 的本地实现必须可用。
- `InvalidResponse` 是通用响应契约错误，区别于 HTTP 失败。

### Provider 和构造

- 默认 endpoint：`https://openrouter.ai/api/alpha/decisions`。
- OpenRouter registry 默认 model：`~typesafe/jev-latest`；另登记 `typesafe/jev-1.13`。
- 消费者 ID：`openrouter/~typesafe/jev-latest`，保留 model 中的 `~` 和 `/`。
- 工厂通过 `orchest-provider` 暴露；增加 `Registry::decision()` 与注册入口。
- 增加 `decision` feature alias，仅依赖现有 HTTP tier。
- 支持显式 API key、环境变量 key、完整 endpoint override；复用共享 HTTP client。
- 显式 key 优先；默认使用 `OPENROUTER_API_KEY`。自定义 key env 不存在时明确失败，
  不偷偷回落到其他凭证。
- `api_url` 是完整 Decisions endpoint，不能经 chat URL normalizer；不自动把任意
  自定义路径改写成默认路径。
- 请求一次发送全部问题；不做逐题 fan-out、自动重试或问题间隐式依赖。
- OpenRouter provider routing / trace 等扩展字段不在首期公开面内，避免无类型
  extra 字段覆盖 model/state/questions。

### SDK 入口

- Rust：类型化 `Decision::decide`，以及通过 provider wall 构造并执行的 `decide` 便捷入口。
- Python：`orchest.decide(state=..., questions=..., model=..., api_key=...,
  api_key_env=..., api_url=..., timeout_ms=...)`，返回有 TypedDict 声明的原生 dict；网络等待释放 GIL。
- TypeScript：`decide({state, questions, model, apiKey?, apiKeyEnv?, apiUrl?, timeoutMs?})`，
  返回 `Promise<DecisionResponse>`，问答使用 discriminated union。
- 两个绑定只做类型转换和 FFI，provider/model/key 解析与校验留在 Rust。
- Python 和 TS 的结果字段保持协议的 snake_case；配置参数沿用各语言命名惯例。

### 校验和错误

通用校验拒绝数值/bool/null instructions、空问题集合、
空 Choice criteria、空 Score criteria，以及不符合类型的 criteria。嵌套描述内容
可以包含任意 JSON 值；校验针对描述的顶层类型。OpenRouter adapter 单独拒绝数值/bool/null state。OpenRouter schema 对 Score 的
最小长度为 1，SDK 不擅自收紧成 TypeSafe 文档的 2。

成功响应解析后校验问题/答案 ID 集合及类型一致、Choice 选项有效、Boolean 和
已提供的 probability/confidence 在 0–1 范围、Score 在 0..levels-1 范围。
分布存在时校验 key 集合与相应 options/levels 一致。浮点求和校验允许舍入误差；
不要求 score 与概率按某个固定舍入方式精确相等。

无 key 返回 `MissingApiKey`；非法请求 `InvalidRequest`；401 返回 `InvalidApiKey`；
HTTP 错误返回 `ProviderHttpError`；无效 JSON、缺少必填字段或响应契约违规返回 `InvalidResponse`；
传输超时返回 `Timeout`。保留 provider/model/status、Retry-After 秒数和上游错误
信息，复用现有 Python ModelError / TS ProviderError 转换。日志不记录 key、state、
instructions 或完整请求/响应正文。

## Acceptance Criteria

- [x] 不启用 HTTP feature 时，自定义本地 Decision 可经 Registry 选择并执行，允许任意 JSON state 和缺省 usage。
- [x] 公共契约不含 noul 或内置供应商；OpenRouter adapter 单独完成 boolean/probability ↔ noul 映射。
- [x] 用用户的客服示例提交 Boolean/Choice/Score 时，HTTP method/path/auth 和 JSON 均与 OpenRouter 契约一致。
- [x] state/instructions/criteria 使用嵌套对象或数组时，结构未经字符串化传到上游。
- [x] Boolean 的 0.95 返回为浮点概率；三级 Score 的 1.05 返回为浮点分数。
- [x] Choice/Score 的可选字段缺失时保持缺失；完整响应保留分布、置信度、结构化 legend、token usage 和 cost_usd。
- [x] 响应缺少答案、答案类型不符、未知 choice 或数值越界时调用失败，不把不完整判断当作成功结果。
- [x] 非法请求在发送前返回 InvalidRequest；401、429 + Retry-After、5xx、超时、非 JSON 成功响应均有测试。
- [x] Registry 无网络可发现 Decision；默认模型和固定版本正确选择，已选 factory 不被其他 config.model 偷换。
- [x] model 中的 `~` 和 `/` 原样保留，endpoint override 不被拼接为 chat/completions。
- [x] 显式 key、默认环境变量、自定义环境变量和缺失凭证路径均有测试。
- [x] Rust、Python、TypeScript 示例能表达同一批客服判断，并在代码中处理置信度缺省与升级逻辑。
- [x] Python 本地 HTTP 集成测试验证真实扩展调用和结构化错误，且网络等待不占 GIL。
- [x] Node 本地 HTTP 集成测试验证真实 native addon、JS 导出、类型声明和结构化错误。
- [x] cargo test --workspace、cargo clippy --workspace -- -D warnings、cargo fmt --check、scripts/lint-check.sh 通过。
- [x] Python 通过 maturin 构建验证，Node addon 构建及 SDK 测试通过；live 调用若未运行则明确记录。

## Notes

设计依据和范围见 [PRD](../../prd.md)。不增加 runtime 自动调用或默认分流策略。
