# 001 · 异步运行对象与事件迭代实施计划

## 先读

- `crates/orchest/src/run/{mod,handle,delivery,supervisor}.rs` 和 `delivery_tests.rs`。确认主事件接收器、背压、终止事件与核心任务结束各自的边界。
- `crates/orchest-{py,node}/src/{lib,asr_stream}.rs`。核对旧入口的运行句柄槽位，以及已有的 PyO3 future 和 napi 异步接线。
- `python/orchest/{__init__.py,__init__.pyi}`、`js/{index.js,native.d.ts,public.d.ts}` 和两端 SDK 测试。

## 修改范围

- 两端绑定的启动入口与异步运行对象、Python 包导出及 stub、Node JS 包装及声明、双端示例和测试。
- 如需拆分运行控制与等待核心任务的所有权，在 `crates/orchest/src/run/handle.rs` 扩展可供两端复用的机制。

## 实施顺序

1. 先写两端最小调用示例，借此确认公开签名。建议 Python 用 `await agent.start_run(input, messages=...)`、`async with run`、`async for event in run`、`await run.wait()` 和 `await run.aclose()`。Node 用 `await agent.startRun(input, messages)`、`for await (const event of run)`、`await run.wait()` 和 `await run.close()`，迭代器 `return()` 走同一关闭路径。同步确定启动冲突和过早等待的公开错误类型、机器可读错误标识及导出，让调用方按类型或标识处理这两种情形。
2. 两端各用一个 Agent 占用机制覆盖旧同步、回调和新异步入口。在调用核心启动前原子占用，初始化失败时回滚。旧 Agent 控制方法通过当前占用访问同一运行，旧入口继续返回原有列表、Promise 或回调结果。运行完成时只清理属于该运行的占用。等待核心任务和调用宿主回调时释放槽位锁。
3. 新运行直接消费 `AgentRun` 返回的主接收器。每次宿主拉取只等待并转换一个事件，串行访问接收器并确保关闭能唤醒挂起的读取。复用已有 PyO3 future 桥和 napi 异步方法。过早调用 `wait` 立即返回本项定义的错误，接收器仍可继续使用。核心任务的收尾由运行状态持有，调用方取消自己的等待时可以再次等待。
4. 用已观察到的 EOF 判断耗尽，并在核心任务结束后由 `wait` 返回整次根运行的结果。完整向宿主交付每次尝试的事件。根运行发出 `RunRestarted` 时清除上一轮的候选失败，后续由最终尝试的完成、失败或取消决定结果。重试耗尽时保留核心发出的最终失败或中止结果。按核心事件来源区分根运行和转发的子运行事件。
5. 把显式关闭、Python 上下文退出、Node 迭代器提前退出和首次读取失败接到同一清理路径。活跃运行先请求取消，再释放接收器并清理读等待，最终由核心任务结束释放 Agent 占用。单独取消运行时保留接收器，让调用方继续读取终止事件。覆盖启动失败和旧回调出错后的占用释放。
6. 用可暂停的本地模型 fixture 验证事件循环活性、满队列取消与关闭、首次读取失败、重复关闭、同 Agent 入口竞争和不同 Agent 并行。将事件序列与 Rust 基线逐项对比。断言启动冲突的错误类型或标识，并分别在未迭代、队列已满、已收到终止事件但尚未读到 EOF 时调用 `wait`，验证均立即返回同一类可恢复错误，随后仍能读完并取得结果。用可重启的根运行验证失败、重启、成功的事件顺序和最终成功结果，再跑一次重试耗尽并断言最终失败结果。
7. 在同机用固定模型事件数、负载、通道容量和消费节奏重复测量 Rust、Python、Node。记录环境和基线波动，比较首事件延迟、事件吞吐、队列峰值、取消响应时间，并测持续消费、暂停恢复和满队列退出。用 `event_delivery_stats` 核对核心队列，记录有无额外绑定积压及主要跨语言开销。
