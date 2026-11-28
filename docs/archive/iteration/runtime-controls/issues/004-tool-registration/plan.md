# 004 · Node 工具注册对齐与 SDK 验收实施计划

## 先读

- `crates/orchest-py/src/lib.rs` 的 `register_agent_tool`、`register_write_file_tool`，`crates/orchest-node/src/lib.rs` 的现有注册入口，以及 `crates/orchest/src/tool/` 的工具实现。
- `js/{index.js,native.d.ts,public.d.ts}`、`examples/{python,typescript}/agents/`、`docs/guide/sdk-{python,typescript}.md`、`CHANGELOG-SDK.md`。
- [开发流程](../../../../../../WORKFLOW.md)、`.github/workflows/sdk.yml` 和 [0.3.0 发布计划](../../../../../releases/0.3.0.md)。

## 修改范围

- Node 绑定注册方法与公开类型。将 Python 当前包含的双端共享构造规则归入 Rust 共享实现。
- 双端宿主示例、集成测试、SDK 指南和 SDK changelog。将实际验收基线交给发布说明。

## 实施顺序

1. 对照 Python 固定 Node 注册的入参和默认值。Agent-as-Tool 覆盖默认 `question` 和自定义输入键，保持现有 schema、输入校验和结果提取行为。写文件工具沿用 Rust `WriteFileTool` 构造器、effects 和默认审批，将两端共用的注册转换放在 Rust 共享层。构造子 Agent 时使用其配置、模型和已注册工具。
2. 挂到 Node 现有 ToolRegistry 构建路径，补公开声明。用本地模型 fixture 验证子 Agent 收到正确输入并返回结构化结果。在临时目录检查写文件允许和拒绝时的审批事件、工具结果及文件内容。跑 schema、handler、异步轮询工具注册回归。
3. 在两个已安装的 SDK 上运行同一宿主流程。新建持久化运行并异步消费事件，按请求 ID 回复审批，读到 EOF 后取得结果，重建宿主，再按 Session ID 恢复并追加输入。复用两端示例和共享 fixture。旧同步、回调、事件返回和工具注册仍由回归测试覆盖。
4. 本地跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`、`bash scripts/lint-check.sh` 和事件声明检查。按工作流构建 wheel 及 npm 主包与本机平台包，从仓库外安装，运行 Python pytest、Node 测试，并用 `npm run typecheck` 验证公开声明。合并前查验 Linux x86_64、Linux arm64、macOS arm64 的已安装包测试。
5. 在双端指南说明旧调用的迁移方式及新入口的消费、等待、提前退出和恢复顺序。在 `CHANGELOG-SDK.md` 记录已验证的用户可见变化。汇总四项验收、性能数据与三平台结果，记录验证所用 Rust 版本、源码提交及构建命令，供选择 0.3.0 候选范围时引用。
