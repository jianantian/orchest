# 003 · Session v2 配置与恢复实施计划

## 先读

- `crates/orchest/src/session/{store,sqlite,log}.rs`、`crates/orchest/src/run/{mod,config,actor,supervisor}.rs`、`checkpoint_tests.rs` 和 `examples/rust/resilience/session_persist_resume.rs`。
- 两端绑定的 Cargo features、001/002 的启动与控制入口、`.github/workflows/sdk.yml` 和 [SDK Session 证据 PRD](../../../sdk-session-evidence/prd.md)。

## 修改范围

- `orchest` 会话层的 SQLite 写入协调及运行生命周期接线、绑定的本地存储配置和恢复入口、公开声明、双端恢复测试和示例。
- 两端绑定启用 `sqlite-session`，在现有打包矩阵验证构建与加载。

## 实施顺序

1. 用两个宿主示例确定 SQLite 路径、Session ID 的配置方式和恢复签名。区分只恢复和追加新输入。为持久化启动和恢复定义类型化的调用错误，覆盖缺失配置、存储无法打开、初始化或读取、写入冲突、新建时会话已存在、恢复时会话不存在。开始执行后的持久化失败沿用核心运行事件。
2. 先在 Rust SQLite 层验证按数据库身份和 Session ID 加锁的方案。用独立连接检验同一文件经不同路径打开时的合流。优先验证每会话的非阻塞 OS 文件锁与进程退出自动释放，核对支持平台、MSRV 和可用的安全 Rust 接口。锁身份要在参与协调的 SDK 进程中一致，独立 Session 的模型等待与写入仍可并行。
3. 为 SDK 的持久化入口在 Rust 中接入可失败的准备阶段。打开并初始化 SQLite，取得同一会话的写入权，再检查新建目标是否已有会话日志，或在恢复时通过 `load_session_log` 读取快照。准备阶段的失败直接返回给启动或恢复调用。复用核心对会话是否存在的判定，保留现有 Rust `AgentRun::start_with_messages` 签名。准备成功后把写入权从准备阶段移交给核心运行，连续持有至最终会话入账或失败退出。覆盖宿主取消、事件接收器提前关闭和宿主进程异常退出。
4. 用同一配置构建器装配模型、工具和 SessionStore，将显式恢复委托给 `AgentRun::resume` 或 `resume_with_input`。沿用 001 的事件接收器及 002 的控制和错误类型。核对两个路径的会话投影、追加的用户轮次、checkpoint 和未完成副作用的恢复行为。
5. 用临时数据库和同步屏障跑独立连接与子进程竞争。覆盖启动对启动、启动对恢复、恢复对恢复，暂停最终入账时尝试第二次取得写入权。断言冲突方在调用模型、派发工具或追加会话日志前得到类型化错误。验证不同 Session 可独立推进，不同数据库的同名 Session 互相隔离，初始化失败和杀掉持有者后均能重新取得写入权。正常退出后，先用同一 Session ID 尝试新建并断言会话已存在、日志原样保留，再用重建的宿主恢复并追加输入。
6. 固定日志长度、checkpoint 位置和宿主配置，记录同机多次恢复的延迟与峰值内存。用本地构建产物完成轮测，再交给 004 的三平台集成验收。
