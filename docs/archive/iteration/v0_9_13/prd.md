# v0.9.13 Spec：core/node/py 改名收尾

## 背景

v0.9.12（Provider 统一）把 provider 层全部迁到 `orchest-*` 命名（`orchest-protocol`、`orchest-provider-core`、`orchest-provider-http/-stream/-visual`、`orchest-provider`）。ADR-0001 Decision 2 当时明确把非 provider 层的改名列为「follow-up, not a blocker」：

> Renaming the non-provider crates (`core/node/py`) is a follow-up, not a blocker.

`agent-runtime-core` / `agent-runtime-py` / `agent-runtime-node` 三个 crate 因此保留旧名，ADR 的「To revisit」清单里也记了这一项，但没有排期。v0.9.13 就是把这个 follow-up 做掉，closes 该 revisit 项。

这三个 crate 均未发布（无 `pyproject.toml`/`package.json` 面向 PyPI/npm 的历史版本，仍是 0.1.0），改名没有对外破坏性。

## 目标

把 `agent-runtime-core/py/node` 及其对外可见的 SDK 包名全部迁到 `orchest-*` 系列命名，与 provider 层保持一致，closes ADR-0001 的 revisit 项。

## 范围

| 旧名 | 新名 |
|------|------|
| crate `agent-runtime-core` | `orchest` |
| crate `agent-runtime-py` | `orchest-py` |
| crate `agent-runtime-node` | `orchest-node` |
| Python 包/目录 `python/agent_runtime/`（`import agent_runtime`） | `python/orchest/`（`import orchest`） |
| PyO3 native submodule `agent_runtime.agent_runtime_py` | `orchest.orchest_py` |
| npm 包 `@orchest/agent-runtime` | `@orchest/sdk` |
| crate `orchest-providers`（umbrella facade） | `orchest-provider`（与 `orchest-provider-{core,http,stream,visual}` 前缀一致） |
| 原生产物文件名 `agent_runtime_node.node` | `orchest_node.node` |

改动内容：
- `git mv` 三个 crate 目录 + `python/agent_runtime` → `python/orchest`
- 根 `Cargo.toml` workspace members、`orchest-py`/`orchest-node` 的 path 依赖、三个 crate 自身 `Cargo.toml` 的 `name`
- 所有 `use agent_runtime_core::` / `use agent_runtime_py` / `use agent_runtime_node` 引用（crate 源码 + rust/python/typescript examples + 测试）
- `pyproject.toml`（`name`、`module-name`）、`package.json`（`name`、`files`、`build:native` 脚本）、`js/index.js`、`scripts/copy-node-addon.mjs`
- 当前有效文档：`README.md`、`AGENTS.md`、`docs/guide/{quickstart,sdk-python,sdk-typescript}.md`（仅涉及三个 crate 本身的引用；不涉及旧 provider 层 deprecated crate 名的引用，那是另一个独立的文档滞后问题，不在本次范围）
- `docs/adr/0001-provider-unification.md` 的 revisit 项标记 resolved

## 不在范围内

- provider 层命名（v0.9.12 已完成，不动）
- ~~`AGENTS.md`/`README.md`/`docs/guide/quickstart.md`/`docs/polaris/observability.md` 里残留的、指向已在 v0.9.12 删除的旧 provider crate（`agent-runtime-model`/`agent-runtime-providers`/`agent-runtime-{aigc,asr,tts,realtime}-providers`）的过期引用——这是 v0.9.12 收尾时文档没跟上的独立问题~~（作为本迭代的追加提交一并修完，见下方"追加：v0.9.12 文档债清偿"）
- 发布准备（crates.io / PyPI / npm 正式发布）——两个 SDK 包仍是 0.1.0 未发布状态，不受影响
- `docs/archive/**` 下的历史迭代记录——保持原样，不做回溯性改名（会破坏历史记录的准确性）

## 验收标准

- [ ] `cargo check --workspace --all-features` 通过
- [ ] `cargo check -p orchest --examples` 通过（rust examples 全部编译）
- [ ] 全仓库(排除 `docs/archive/**`、`docs/research/**`、`docs/review/**`、`docs/todo/**`)不再出现 `agent-runtime-core`/`agent-runtime-py`/`agent-runtime-node`/`agent_runtime_core`/`agent_runtime_py`/`agent_runtime_node`，以及裸 `agent_runtime`（Python 包名）
- [ ] `docs/adr/0001-provider-unification.md` 的 revisit 项标记为 resolved

## 测试策略

| 验证项 | 方法 |
|--------|------|
| Rust workspace 编译 | `cargo check --workspace --all-features` |
| Rust examples 编译 | `cargo check -p orchest --examples` |
| 无残留旧引用 | 全仓库 grep（排除 archive/research/review/todo） |

## 依赖

- v0.9.12（Provider 统一）已完成，本迭代是其 ADR revisit 项的收尾

## 追加：v0.9.12 文档债清偿

原本列在"不在范围内"、建议另开工单的 v0.9.12 文档滞后问题，实际在本迭代内一并处理了：

- `AGENTS.md`：workspace structure 代码块、"Locked Design Decisions" 里过期的 provider 独立 crate 规则、开头的 stage 描述，全部改为反映当前的 weight-tier 架构（`orchest-protocol` + `orchest-provider-core` + `-http`/`-stream`/`-visual` + `orchest-provider` 墙）
- `README.md`：workspace structure 代码块同步
- `docs/guide/quickstart.md`：Cargo 依赖片段与 provider 构造代码改为 `orchest-provider`/`orchest_provider::create_adapter_from_config`（与 `examples/rust/basic_agent_run.rs` 实际代码保持一致）
- `docs/polaris/observability.md`：两处 `agent-runtime-providers` 引用改为 `orchest-provider`
- `examples/rust/providers/{deepseek,openrouter}.rs`：doc comment 里的 `cargo run -p agent-runtime-providers` 改为 `cargo run -p orchest`（examples 实际注册在 `orchest` 的 `[[example]]`，不是旧 provider crate）
- `docs/iteration/v0_10/prd.md`（规划中、未执行）：`AsrProvider`/`TtsProvider`/`agent-runtime-aigc-providers` 等过期 trait/crate 名改为当前的 `Asr`/`Tts`/`VoiceManager` trait + `orchest-provider-visual`
- `docs/iteration/v0_10/issues/005-multimedia-ingestion-audio-output.md`：`FakeAsrProvider`/`FakeTtsProvider` 的"pre-seeded finding"经核实后，实际测试替身已变成 `crates/orchest-provider/tests/selection.rs` 里私有的 `FakeChat`/`FakeAsr`，且当前没有 `FakeTtsProvider` 对应物——标注为"需在 v0.10 开工前重新核实"，而不是直接改名字了事

`docs/adr/0001-provider-unification.md` 的 Context 段落（描述 v0.9.12 之前"五个 crate 按模态拆分"的旧状态）和 `docs/iteration/roadmap.md` 的"已完成"历史行（描述每个迭代当时交付了什么）**不改**——那是准确的历史记录，不是过期的当前状态声明。
