# SDK Session 证据 · DSH 轨迹

## 合入记录

2026-11-28 合入 `main`，对应 [PR #370](https://github.com/jianantian/orchest/pull/370) 和 [Issue #369](https://github.com/jianantian/orchest/issues/369)。[Rust CI](https://github.com/jianantian/orchest/actions/runs/46148139577) 和[三平台 SDK 安装验收](https://github.com/jianantian/orchest/actions/runs/46148139532)均通过。分页和运行延迟已有本地基线，SDK 稳态队列峰值待补测。

## 目标

把 Rust 已有的本地 Session 证据接入 Python / Node。宿主可为单次运行显式开启采集，并按会话分页排查模型调用的尝试和重试。

## 范围与依赖

- 两端公开尝试元数据与记录缺失状态，沿用现有 Rust 证据层的会话隔离与保留规则。授权的完整输入重建继续由 Rust 提供。界面与远程上传留待后续规划。
- 接入依赖[运行控制的本地会话恢复](../../archive/iteration/runtime-controls/issues/003-session-resume/spec.md)。在共享存储与会话身份的契约确定后，可继续推进轨迹绑定。
- 读取句柄固定到授权时的会话实例。删除、同名重建或撤销后，旧句柄返回明确的失效状态。查阅示例展示如何按尝试 ID 关联记录、如何用下一页游标继续，以及如何解释未知结果。

## 架构与性能

实例校验、字段白名单和分页语义集中在 Rust 证据层。绑定映射类型化结果。关闭采集时沿用原运行路径。启用时复用有界队列，采集故障独立报告。读取按页推进，限制每次扫描量与分配，避免将整份会话历史载入 SDK。

## 迭代验收

- [x] Python / Node 均通过默认关闭、恢复后显式重新启用、重试分页、会话隔离与元数据白名单验证。
- [x] 证据故障下 Session 仍可恢复和继续运行，Rust 的采集、查询与授权重建通过回归。
- [x] 两端的本地构建产物能运行最小示例，指南记录公开字段和未知状态的含义。
- [ ] 使用同一模型 fixture 对比采集关闭与开启时的运行延迟和队列峰值。用含大量不可公开记录的固定证据库记录分页延迟、扫描量和峰值内存；页大小固定时，单次查询的工作量受限。

详细验收与实现步骤见 [001 规格](./issues/001-sdk-session-evidence/spec.md)和[实施计划](./issues/001-sdk-session-evidence/plan.md)。
