# 008 实现路线

## v0.7 依赖判断

不依赖 v0.7。这个 issue 只收口独立 AIGC provider crate 的构造、telemetry、examples 和全量验证。把 gateway 注册成 core tool 可以在 v0.7 或之后单独做，不是本迭代 blocker。

## 步骤

1. **实现 provider factory**
   - 在 `lib.rs` 或 `factory.rs` 定义 `create_image_provider_from_config(config: AigcProviderRuntimeConfig)`
   - 支持 Crazyrouter、Aliyun、OpenRouter、Renderful
   - `AigcProviderRuntimeConfig` 是应用代码唯一 provider 构造入口
   - provider-specific config 只在 factory 内构造，不要求调用方直接了解各 adapter config

2. **实现 API key resolution**
   - 优先级：显式 `api_key` > 显式 `api_key_env` > provider 默认环境变量
   - 如果配置了 `api_key_env` 但变量缺失或为空，构造失败
   - 这种失败不能 fallback 到 provider 默认环境变量
   - unknown provider 返回 `AigcError { code: "unknown_provider" }`
   - invalid/empty model 返回 `AigcError { code: "invalid_model" }`

3. **验证 shared HTTP client 使用**
   - 检查四个 provider adapter 都调用 crate shared client
   - 移除各 adapter 内单独 `reqwest::Client::new()` 或重复 builder
   - 增加测试或静态检查覆盖 shared client singleton

4. **实现 telemetry helpers**
   - `telemetry.rs` 定义 spans：image create、provider request、provider poll、asset persist、signed URL generation
   - metrics 覆盖 provider duration、asset persistence duration、generated image count、persisted bytes、error counts
   - 默认日志字段不得包含 base64 payload、signed URL、API key、storage credentials

5. **补 examples**
   - 新增 text-to-image URL delivery 示例，使用 local storage 或 mock provider，无真实 provider 凭证也能编译
   - 新增 Base64 delivery 示例，展示仍返回 `asset_id`
   - 新增 old `asset_id` resolve fresh URL 示例
   - 示例中如需真实 provider，必须用环境变量 guard，不能在无凭证时失败

6. **全量测试收口**
   - 跑 Crazyrouter、Aliyun、OpenRouter、Renderful adapter 测试
   - 跑 gateway mock + local storage e2e 测试
   - 跑 public serialization contract 测试
   - 修复 clippy/fmt 问题

7. **验收**
   - `cargo test -p agent-runtime-aigc-providers`
   - `cargo clippy -p agent-runtime-aigc-providers -- -D warnings`
   - `cargo test --workspace`
   - `cargo fmt --check`

## 关键决策

- 008 是收尾 issue，不新增新的核心抽象；如果发现必须新增类型，应先回补到对应前置 issue 的 spec。
- telemetry 只提供 helper 和命名约束，不安装 global subscriber/recorder。
- examples 证明 SDK 可用性，但不能引入对 v0.7 core runtime 的依赖。
