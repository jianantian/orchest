# Implementation Plan — System One Decisions

> 使用 subagent-driven-development 执行有明确边界的绑定子任务；统一审查并提交本 issue。

**Goal:** 提供通用结构化判断能力，OpenRouter 是首个实现。

**Architecture:** protocol 定义 Boolean/Choice/Score 和验证；HTTP adapter 转换
OpenRouter noul wire；provider wall 负责注册、模型选择和原子调用；绑定仅做转换。

**Tech Stack:** Rust async-trait/serde/reqwest，PyO3，napi-rs；无新依赖。

## Global Constraints

- 不修改 agent loop；不在公共契约中暴露 noul 或默认 OpenRouter 模型。
- 消费者只依赖 protocol + provider wall；绑定没有业务判断。
- 一 issue 一 commit，所有实现和文档统一提交；不提交凭证和 live 原始数据。

## 固定接口与验证顺序

1. `DecisionRequest { state: Value, questions: BTreeMap<String, DecisionQuestion> }`。
   `DecisionQuestion` 采用 `#[serde(tag = "type", rename_all = "snake_case")]`，
   variants 为 `Boolean { instructions: Value, criteria: Option<BooleanCriteria> }`、
   `Choice { instructions: Value, criteria: BTreeMap<String, Value> }`、
   `Score { instructions: Value, criteria: Vec<Value> }`。
   BooleanCriteria 的 Rust 字段 true_ / false_ 对应 JSON true / false。
2. `DecisionAnswer::Boolean { probability: f64 }`；Choice 与 Score 字段遵循 spec。
   `DecisionResponse { model: String, answers: BTreeMap<String, DecisionAnswer>,
   usage: Option<DecisionUsage>, id: Option<String>, provider: Option<String> }`；
   `DecisionUsage { input_tokens: u64, output_tokens: u64, cost_usd: Option<f64> }`。
3. `DecisionRequest::validate()` 返回 InvalidRequest；
   `DecisionResponse::validate_for(&request)` 返回 InvalidResponse。
4. wall `DecisionConfig { model: String, api_key: Option<String>, api_key_env: Option<String>,
   api_url: Option<String>, timeout_ms: Option<u64> }`，`DecisionConfig::new(model)`。
   `Registry::create_decision(&DecisionConfig) -> Result<Box<dyn Decision>, ProtocolError>`；
   顶层 `create_decision(&DecisionConfig)`、
   `async decide(&DecisionConfig, DecisionRequest) -> Result<DecisionResponse, ProtocolError>`。
5. Python `decide(*, model, state, questions, api_key=None, api_key_env=None,
   api_url=None, timeout_ms=None)` 同步返回 dict，网络等待释放 GIL；
   TS `decide({model,state,questions,apiKey?,apiKeyEnv?,apiUrl?,timeoutMs?})`
   返回 Promise，结果字段为 snake_case；两个入口共用 wall 的配置和验证。

- [ ] Protocol：先添加公开合同测试，运行 `cargo test -p orchest-protocol --test decision`，
  确认新契约缺失使测试失败；实现类型和验证后通过。
- [ ] Provider：先添加本地 HTTP 合同及无 HTTP 的自定义 Decision 注册测试，运行
  `cargo test -p orchest-provider --features decision --test decision`，再补实现。
- [ ] Bindings：先写 `python/tests/test_decision.py`、`js/tests/decision.test.cjs`；
  验证缺失导出失败后补 FFI、声明和包装，构建真实扩展后测试。
- [ ] 文档与发布检查：补三语言示例，运行完整 workspace 检查，独立审阅最终 diff。

## Files to read

- `AGENTS.md`、`CONVENTIONS.md`、`WORKFLOW.md`、本 issue spec 与 PRD。
- `crates/orchest-protocol/src/{descriptor,error,lib}.rs`。
- `crates/orchest-provider/src/{registry,facade,lib}.rs`。
- `crates/orchest-provider-http/src/{lib,http}.rs`、`providers/openrouter/`。
- `crates/orchest-provider-core/src/{http,registry}.rs`。
- `crates/orchest-{py,node}/src/{atomic,error,lib}.rs`。
- `python/orchest/__init__.{py,pyi}`、`js/index.{ts,js,d.ts}`、`js/native.d.ts`。
- OpenRouter OpenAPI 中 Decisions endpoint 及 request/answer schemas。

## Files to change

- `crates/orchest-protocol/src/decision/`：类型、校验和测试；lib/descriptor 接线。
- `crates/orchest-provider-http/src/decision/`：HTTP 适配、注册与本地服务测试；lib 接线。
- `crates/orchest-provider/src/decision.rs`、registry/lib/Cargo.toml：公共构造、便捷 API、能力桶和 feature。
- `crates/orchest-provider/tests/`：能力发现与配置选择测试。
- Py/Node `src/decision.rs`、lib.rs；Python 与 JS 公共导出、类型和测试。
- `examples/{rust,python,typescript}/providers/` 的 Decisions 示例及必要的 Cargo example 声明。
- `docs/guide/`、本迭代文档、`docs/iteration/roadmap.md`。

## Steps

1. 确认设计，在 GitHub 创建本 issue；使用 `iteration/v0_17` 独立 worktree。
2. 保存精简的官方请求/响应 fixture（注明来源和日期），先写类型及校验的失败测试。
3. 实现 Decision trait、问答和 usage 类型；通过结构化输入及可选输出测试。
4. 实现 HTTP adapter；本地 TCP/HTTP fake 验证 path、headers、单次批量请求、错误和 timeout。
5. 接入 registry、默认与固定模型、共享配置解析及 Rust 便捷入口；验证 provider wall 和 feature。
6. 增加 Python/Node FFI 与公共声明；用真实本地 addon/extension 驱动 HTTP fixture。
7. 补三语言示例和文档；检查问答语义、配置和错误在三语言一致。
8. 运行针对性检查，再运行 spec 中完整 workspace/SDK 检查；区分既有失败与本次回归。
9. 自查 diff、更新验收状态；以 `feat:` 提交，提交信息包含 `closes #N`。
10. 按仓库工作流准备 PR；live evidence 单独记录，合并须遵守仓库 review 约定。
