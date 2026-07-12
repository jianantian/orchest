# Issue 010:显式协议选择 — 消费者面 + 文档 + binding 回归

Parent: [ADR-0002](../../../../adr/0002-protocol-provider-decoupling.md) Phase 2 · AFK · 依赖 009

## 现状

008 让 `provider/protocol/model` 显式形可解析,009 把 Elss 溶解成真正的多协议 provider。
但 ADR Problem 4 的用户故事("同一 model,多个协议")此刻只在测试里被覆盖,消费者无从得知
这个能力存在:`.env.example` 只列旧形,catalog 的 `model_id_format`/`model_id_example` 提示
未反映协议维度,node/py 也没验证过新字段的向后兼容与错误信息。本 slice 把显式协议选择做成
**一等、可发现**的能力并收口消费者面。

## 方向(本 slice 建什么)

- **用户故事可用且被文档化**:`elss/messages/claude-sonnet-5`(走 Anthropic Messages,保
  prompt caching)对比 `elss/chat/claude-sonnet-5`(走 OpenAI Chat,tool-use parity)——
  两者都能构造出对应协议的 adapter,加一个 example/集成测试佐证。
- **`.env.example` 更新**:`examples/demo/*/.env.example` 在现有 Elss 两行旁补上 canonical
  `provider/protocol/model` 形(`elss/messages/…` / `elss/chat/…`),并注明旧的
  `elss/anthropic/…` `elss/openai/…` 仍作别名可用。
- **catalog 提示更新**:多协议 provider 的 `model_id_format`/`model_id_example` 反映
  `provider/[protocol/]model`;单协议 provider 不变。
- **binding 回归 + 错误信息**:确认 `orchest-node`/`orchest-py` 经 `normalize_provider_model`
  的路径在新字段下向后兼容;若 provider 不支持所请求的显式协议,错误信息清晰(复用 008 的
  报错)。不新增 binding 参数面(暴露协议维度到 SDK 入参是 post-hotfix 工作)。

## 落地与测试

- 集成测试/example:同一 Claude model 经 `elss/messages/…` 与 `elss/chat/…` 分别落到
  Messages 与 Chat 协议。
- `.env.example` 与 catalog 提示更新后,`cargo test -p <demo>` 与 catalog 相关测试通过。
- node/py 构建 + 测试通过;显式协议不支持时错误信息断言。

## 验收标准

- [ ] "同一 model 多协议"用户故事有 example/测试佐证并可运行
- [ ] `.env.example`(相关 demo)与 catalog `model_id_format`/`model_id_example` 反映协议维度,
      旧别名形注明仍可用
- [ ] node/py 向后兼容回归通过;不支持的显式协议错误信息清晰
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` 全过

## 非目标

- 不在 Python/TS SDK 入参层新增协议选择面(post-hotfix)。不实现 `Protocol::Responses`。
