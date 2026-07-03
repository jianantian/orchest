# Issue 005:`orchest-provider` 提供可复用 fake `Asr`/`Tts`

GitHub: [#196](https://github.com/jianantian/orchest/issues/196) · release-blocker · 无依赖

## 现状

全 workspace 无可复用的 fake `Asr`/`Tts`:唯一的 `FakeAsr` 是 `crates/orchest-provider/tests/selection.rs` 里的私有 struct(集成测试二进制,下游永远 import 不到);`FakeTts` 为零。`examples/demo/briefing-desk/src/media.rs` 被迫从零手写两个真实的 `orchest_protocol::{Asr, Tts}` trait impl。

## 方向(提案)

采用 issue 内"随 provider crate 发布共享 fake"路线:

- `crates/orchest-provider` 新增 `pub mod fakes`,置于 `testing` Cargo feature 之后(`#[cfg(feature = "testing")]`),不进默认 feature,不污染生产依赖面
- `fakes::FakeAsr` / `fakes::FakeTts` 实现 `orchest_protocol::{Asr, Tts}`,行为确定性(固定/可注入的 transcript 与合成字节),形态以 demo `media.rs` 现有两个 impl 为蓝本上移
- demo 的 `FakeAsr`/`FakeTts` 改为依赖 `orchest-provider = { features = ["testing"] }`,删除本地手写版

**`selection.rs` 里的私有 `FakeAsr` 不动**:那是一个 `(&str, &str)` 元组,
`transcribe()` 直接 `unreachable!()`,只用于"按 `CapabilityDescriptor` 路由选型"
的测试占位,和共享 fakes 模块要提供的"离线可用、确定性转写/合成"是两个不同
目的的 test double。#196 的诉求是"下游缺可复用 fake",不涉及 `selection.rs`
自己这份路由测试用的最小占位,没必要强制统一,徒增耦合。

不选"文档化让下游自己写"路线:验证报告与 #196 的定性是 release-blocker-eligible("missing fake provider hook"),v1.0 冻结后下游做离线测试的第一件事就是找 fake,应由 provider crate 自带。

## 落地与测试

- fake 自身的单元测试(确定性输出、trait 契约)
- demo 切换后 `cargo test -p briefing-desk-demo` 全绿,输出贴回 #196 或关闭它的 PR
- `orchest-provider` crate 文档说明 `testing` feature 的用途

## 验收标准(对齐 GitHub #196)

- [ ] `fakes` 模块 + `testing` feature 落地,`FakeAsr`/`FakeTts` 可被下游 import
- [ ] demo 改用共享 fake,本地手写版删除(`selection.rs` 的路由测试占位保持不动)
- [ ] demo 测试重跑,输出贴回 issue/PR
- [ ] `docs/review/v0_10_demo_validation.md` 更新(Triage #2 行)
