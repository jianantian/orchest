# 001 · 实施计划

## 先读

- [Session 证据 PRD](../../../../archive/iteration/session_evidence/prd.md)与[运行控制 003](../../../../archive/iteration/runtime-controls/issues/003-session-resume/spec.md)。
- `crates/orchest/src/session/evidence/{binding,records}.rs`、`crates/orchest/src/run/config.rs` 及现有证据回归。
- `crates/orchest-py/`、`crates/orchest-node/` 中的绑定入口、Cargo feature、类型声明和测试，以及 `.github/workflows/{sdk,release-sdk}.yml`。

## 修改范围

- 两个绑定 crate 的 feature、运行配置、读取入口及公开类型。共用元数据投影由 Rust 证据层提供。
- 两端合约测试、SDK 安装验证，以及 [Python 指南](../../../../guide/sdk-python.md)和 [TypeScript 指南](../../../../guide/sdk-typescript.md)。

## 步骤

1. 在 003 完成后，验证两端打包支持核心证据 feature，并将逐次运行的显式配置接到现有会话实例绑定和 `AgentConfig`。验证新建和恢复的默认行为。
2. 在 Rust 证据层固定读取实例，统一检查实例、游标、限额和撤销。复用按页读取通路，投影允许的记录，保持游标推进。两端映射同一组错误与未知状态。
3. 用相同的本地数据库场景验证两端，覆盖重试、空页继续、跨实例游标、双向实例隔离、撤销、含模拟授权内容的旧库和证据故障下的恢复。
4. 对比开关采集时的运行延迟与队列峰值，测量固定页大小下的扫描量、延迟和峰值内存。从本地构建产物验证两端公开 API，更新指南与迭代验收结果。

## 本地验收

macOS arm64，Rust 1.99.1、CPython 3.14.8、Node 22.22.2。Rust 工作区 1,384 项通过。已安装同一工作树构建的 Python wheel 和 npm 包，Python 35 项、Node 24 项通过。三平台安装验收由 SDK 工作流执行。

### 性能基线

使用共同的 `sdk-runtime-events.json` SSE 完成响应，运行前复用 Agent 和证据库对象。每轮依次交替执行 120 次关闭和 120 次开启采集的新会话，同机测三轮。运行时间从启动到消费事件并取得结果。分页使用 64 或 16,384 条含 1 KiB 模拟内容的可过滤记录，分别执行 300 次首页查询，原始页大小固定为 16。

| 测量（各轮 p50 的范围） | Python | Node |
| --- | ---: | ---: |
| 新运行，关闭采集 | 2.414–2.515 ms | 2.102–2.256 ms |
| 新运行，开启采集 | 2.227–2.311 ms | 2.022–2.104 ms |
| 64 条库的分页 | 0.243–0.247 ms | 0.204–0.206 ms |
| 16,384 条库的分页 | 0.241–0.245 ms | 0.204–0.209 ms |
| 进程峰值 RSS，64 条 | 85.3–86.2 MiB | 99.4–99.8 MiB |
| 进程峰值 RSS，16,384 条 | 87.6–88.5 MiB | 99.7–100.1 MiB |

运行延迟包含 SQLite、调度和本地 HTTP，数据用于记录本机基线。峰值 RSS 包含宿主、预热和 SQLite 缓存。单页 SQL 按实例和游标索引最多取 17 行，其中最多 16 行投影，第 17 行只用于判断是否还有下一页。Rust 回归用 16,384 条过期记录验证固定大小的页仍会推进，并用容量为 1 的队列和持锁数据库验证丢弃可见、Session checkpoint 和工具仍能完成。默认队列容量为 64 项，SDK 稳态队列峰值尚未实测。

复核补充了两个安装包回归。替换 Session SQLite 文件后，旧授权句柄立即失效。把证据库路径用作 Session 库会返回 `session_storage_error`。Rust 另验证了不同会话仍可并发初始化同一个新库。
