# 005 · 原生权限模式表面与执行后端注入点

GitHub issue: #335

## Background

Multivac 的权限档要映射到"Runtime 的原生模式"（其 ADR-003 §4.1），而 Orchest 现在没有可映射的原生模式：

- `ToolMetadata` 只有 `side_effect: bool`（`crates/orchest/src/tool/metadata.rs:10`）——只有"有没有副作用"，没有"副作用是什么"；
- 判定只有工具级 `Approval::{Never, WhenRisky, Always}` 与 run 级 `ApprovalMode::{PerTool, None, All}`（`crates/orchest/src/run/config.rs:254-262`）——没有命令模式级与路径级；
- 没有批准作用域（一次性 / 会话级）；
- 唯一执行器 `BareSubprocessExecutor` 不做任何隔离（`crates/orchest/src/skill/executor.rs:53-115`），skill 声明的 capabilities 只解析不执行；
- MCP 工具默认 `approval: Never`（`crates/orchest/src/tool/mcp.rs:470-471`），绕过默认审批。

业界的判定粒度谱系（调研 §4）：工具级（dsh）→ 工具级 + 命令模式级（Codex execpolicy、OpenCode、percho、jcode、openhuman、Vercel open-agents）→ 再加路径级（OpenCode `external_directory`、percho、deepagents、jcode `paths.rs`）→ 再加网络/凭据域（OpenCode `webfetch`、openhuman trusted roots）。批准作用域：一次性 / 会话级（Codex `ApprovalStore`、OpenCode always-per-session）/ 持久化。**read-only 的硬保证全部来自 OS 沙箱**（Codex seatbelt/landlock、Claude Code bwrap、openhuman Local/Docker），只做分类的系统都只能算 soft——jcode 自述"defense in depth, not a sandbox"，dsh-purge 靠 patch 本机包就能剥离进程内审批门与沙箱。

ADR-0005 D2 已定：Orchest 提供原生权限模式表面，**档位到效果的映射属宿主**；M1/M2 阶段只读标注 soft，M4 宿主注入沙箱后转 hard。

## Goal

给宿主一套能直接映射的权限原语：结构化的工具效果、单一决策点、命令模式级与路径级规则、批准作用域、默认 fail-closed，以及一个沙箱/执行后端注入点与可信的 hard/soft 事实。

## Acceptance Criteria

- [ ] 工具效果是结构化分类，取代 `side_effect: bool`：至少区分 read、write-workspace、write-outside、network、credential、destructive、process。
- [ ] 权限判定只有**一个决策点**，输出三态：允许 / 询问（带 kind 与载荷）/ 拒绝（带原因）。工具、MCP、子 run 都走它。
- [ ] 规则可表达：工具级、命令模式级（glob）、路径级（glob）；宿主提供规则，runtime 提供匹配器。
- [ ] 未匹配或效果未知的调用**默认询问**（fail-closed），不默认放行。
- [ ] 批准作用域支持两档：一次性、会话级（会话级按"工具 + 归一化参数摘要"记忆，可由宿主选择是否记忆）。
- [ ] MCP 工具不再默认绕过审批：其效果与批准策略与其他工具一致。
- [ ] 存在执行后端注入点：宿主可注入带隔离的执行后端，runtime 暴露**可信的**执行强度事实（hard / soft / none），供界面如实标注。
- [ ] runtime 明确声明"只读能保证到什么程度"：没有注入隔离后端时，只读不得被表述为硬保证。
- [ ] skill 声明的 capabilities 与工具效果的关系被写明：能力声明是提示，不是执行边界（除非由宿主注入的后端执行）。
- [ ] 测试覆盖：四档典型规则的判定、默认 fail-closed、会话级记忆、MCP 工具不再绕过、注入后端后强度事实变化。

## Blocked by

004（询问要走新通道）。

## Notes

- 效果分类是 runtime 词汇，**宿主负责把权限档映射到效果**；runtime 不认识"只读/写前问"这些产品档位。
- 命令模式匹配器可以借鉴 Codex execpolicy / OpenCode 的形态（最后命中规则胜出），但**规则内容由宿主给**，runtime 只提供匹配能力。
- 沙箱后端本迭代只做接口与强度事实，具体实现属 M4。
