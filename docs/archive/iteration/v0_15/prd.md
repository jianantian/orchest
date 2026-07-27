# v0.15 PRD: Gen 协议与子代理语义(Gen Protocol & Sub-Agent Semantics)

## 背景

SDK 优化计划(`docs/todo/2026-07-18-sdk-optimization-plan.md`)主题 B1 / E / C5-C7,叠加
seam-findings Finding 2(`examples/demo/music-gift/docs/seam-findings.md`)。music-gift 实测暴露:

- **Gen 协议空转**: `GenRequest.params: Value` 无 schema,Suno 白名单外的 key(genre/tempo/mood/…)
  静默丢弃,demo 第三次 LLM 调用产出 100% 空转;`duration_secs`/`cover_url` 等一等产品数据
  塞进 `diagnostic_metadata` 垃圾袋(Finding 2)。
- **子代理语义脆弱**: child run 失败被包成 `Ok(Structured{details.error})` 照常返回(demo
  countdown 踩坑记录);子代理输出只有纯文本,消费方靠 `strip_code_fences` 脆弱提取。
- **请求/上下文正确性**(C5-C7,全使用方静默中招): Chat 协议静默丢多模态 block;compaction
  可拆散 ToolUse/ToolResult 对(Anthropic 400);工具结果回插 role 串/并行不一致。
- **Tool 一次性调用样板**(G1/G2,SDK 计划第 5 步允许并入): 缺 `ToolContext::oneshot()` 导致
  countdown 手造 8 字段、collect_info 测试复制校验逻辑测副本。

本迭代清偿,使 Gen 协议"参数类型化、产品元数据有归宿、误用可见",子代理"失败即 Err、输出有契约"。

## 目标

1. Gen 协议音乐参数类型化 + 未知 key 可见(warn);产品元数据(cover/duration)有类型化归宿。
2. 子代理失败 = `Err(ToolError)`;输出格式契约由 SDK 层提取/校验/纠正。
3. C5-C7 三处静默/不一致修复,行为可观测。
4. 一次性工具调用零样板;测试走真实代码路径。

## 非目标

- Image/Video 参数类型化(本次只做 `MusicParams`;其他模态留 `params` 逃生舱)。
- SDK-A2(`AgentConfigBuilder` 暴露 model options,留 backlog,是 demo D12 前置)。
- G3(`ChatModel`/`ModelAdapter` 双名收敛,v1.0 前处理)。
- F(trajectory 录制决策,任意时间点可做)。

## Issue 分解

| Issue | 标题 | 来源 |
|-------|------|------|
| 001 | GenRequest 音乐参数类型化 + 未知 key 警告 | SDK-B1 步骤 1+2 |
| 002 | GenAsset 语义角色 + duration 类型化 | SDK-B1 × seam-findings Finding 2 |
| 003 | agent-as-tool 子代理失败返回 Err | SDK-E1 |
| 004 | 子代理输出格式契约 | SDK-E2 |
| 005 | provider/run 正确性批(C5/C6/C7) | SDK-C5/C6/C7 |
| 006 | ToolContext 一次性调用 helper | SDK-G1/G2 |

依赖顺序: 001 → 002(同改 `capability.rs`/`suno.rs`/demo `music_gen.rs`);003 → 004(同改
`agent_as_tool.rs`,004 复用 003 的 Err 语义);005、006 独立。全部按编号串行提交。

## 验收

- 五项检查全绿(test / clippy -D warnings / fmt / lint-check / cargo doc)
- 各 issue spec 验收框全勾
- demo 侧采用(001/002/003/004/006)作为 SDK 改动的实证用例

## 依赖

- v0.14 合入后开工,无文件级耦合。
- 004 的纠正轮次依赖 v0.13 的 `AgentRun::resume_with_input`。
