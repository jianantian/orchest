# Orchest 下一版 · 三端统一版本

Rust、Python 和 JS 从下一版起使用同一个版本号，工作区已统一为 `0.3.0`。PyPI 的 `orchest-py` 和 npm 的 `orchest-sdk` 及平台包保留已发布版本。Rust 下一版继续使用原有包名。

## Rust 发布清理

2026-11-26 已核对 `orchest`、`orchest-protocol`、`orchest-storage`、`orchest-provider`、`orchest-provider-core`、`orchest-provider-http`、`orchest-provider-stream`、`orchest-provider-visual` 均已从 crates.io 删除。注册表 API 和 Cargo 索引均返回 `404`。Git 标签和源码记录作为归档。

新发 Rust 包前核对原包名的冷却期和重新注册状态。两个发布工作流都会检查 Rust 与 SDK 版本一致，SDK 正式上传也要求对应的 Rust 标签指向同一提交。

### 补发 Rust 0.2.0

Rust `0.2.0` 取自已发布的 SDK `0.2.0` 源码，八个 crate 已于 2026-11-28 发布到 crates.io。Python 和 JS 继续使用已发布的 `0.2.0`。[Rust 0.2.0 发布](https://github.com/jianantian/orchest/releases/tag/v0.2.0) · [发布验证](https://github.com/jianantian/orchest/actions/runs/46033342490)

三端的下一版统一为 `0.3.0`。发布时核对 crates.io、PyPI 和 npm 包页面的版本说明。

## 交付范围

| 交付 | 验收 |
| --- | --- |
| **运行控制** | Python 和 Node 能异步运行、读事件、取消、按请求 ID 回复和恢复 Session。现有同步与回调入口通过回归测试。[实施计划](./runtime-controls/prd.md) |
| **语音** | 可查询 ASR/TTS 能力与用量。按实际验证结果交付 TTS，分别记录 Aliyun、Volcengine、MiniMax 支持的双工、Flush 和取消行为。[实施计划](./voice/prd.md) |
| **DSH 轨迹** | Rust 包包含已合入的可选 Session 证据。显式开启本地采集后，一次失败再重试能查到两条独立记录，且恢复继续使用 Session v2。Python 和 Node 的采集与查询作为独立验收项，根据打包和权限测试结果决定随版公开的范围。[原迭代](../../archive/iteration/session_evidence/prd.md) |

## 交付顺序

1. 开发和验证三个交付项，记录 Rust、Python、Node 各自能用的能力。
2. 用同一候选提交构建三端产物，跑三平台安装测试。把通过验证的范围写进同一份发布说明。
3. 先发统一编号的 Rust 候选 crate，再发同一提交、同一编号的 Python 和 JS 候选包。验证公开安装后，再按同样顺序发正式构建。
