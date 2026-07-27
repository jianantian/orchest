# SDK 优化计划(music-gift demo 暴露的缺口 + 全 SDK 生成质量梳理)

> 状态: hotfix 2026_07_18b / v0.13(质量地基)/ v0.14(skill 机制)/ v0.15(Gen 协议与子代理语义:B1 + E1/E2 + C5-C7 + G1/G2)已落地(见 roadmap 已完成表) | 记录于 2026-07-18,2026-07-19 更新,2026-07-26 更新
> 来源: music-gift demo(`examples/demo/music-gift`,定位即"暴露 SDK 缺口")实测梳理,
> 所有问题均经代码核实并附文件:行号;主题 G 来自 demo 架构评审新增的
> [`seam-findings.md`](../../examples/demo/music-gift/docs/seam-findings.md) Findings 3-5(commit `2f4f880`)。
> demo 侧配套计划见
> [`examples/demo/music-gift/docs/optimization-plan.md`](../../examples/demo/music-gift/docs/optimization-plan.md)。
> 排期方式: 按 WORKFLOW.md 拆进 hotfix / 迭代 issue 后,从本文档移出。

## 背景

music-gift 的 guided pipeline(引导收集 → chat 写词 → review 审核 → music prompt 改写 → Suno 提交 → countdown widget)出现系统性生成质量问题。排查后确认:**最大的问题不是 prompt 写得差,而是精心构造的内容根本没送到模型/provider**——而每一处"送不到"背后都是一个 SDK 缺口。另有三个独立发现(provider 请求正确性、skill 机制、子代理语义)与 demo 无关但同样拉低所有 SDK 使用方的质量下限。

主题划分:

| 主题 | 内容 | 对质量的影响 |
|------|------|-------------|
| SDK-0 | 破测试紧急修复 | CI 红线 |
| A | AgentRun 输入契约 | demo 拍平 system+历史的根因 |
| B | Gen 协议与 run 终止语义 | EnrichedPrompt 空转、截断静默 |
| C | Provider 请求正确性 | 全使用方静默中招 |
| D | Skill 机制 | skill 化推广的前提 |
| E | agent-as-tool 子代理语义 | countdown 踩坑 |
| F | trajectory 录制(决策点) | 质量问题无法复盘 |
| G | Tool 调用易用性(seam-findings 3-5) | 一次性调用/测试样板、双名 cast |

---

## SDK-0 · 紧急:provider-core 测试编译失败(P0,hotfix)

**现状**: `crates/orchest-provider-core/src/gen.rs:80` — 测试 `sync_cache_round_trips_submit_poll_fetch` 引用从未定义的 `handle`(缺 handle 构造与 cache 写入),`cargo test --workspace` 编译失败。系 `3cc248a`(TimedText 替换 lrc)改测试时的遗漏,已提交进 `feat/music-gift-demo` 分支。
**建议**: 补上 `GenHandle` 构造与 `cache` 写入行,恢复原测试意图(submit→poll→fetch→fetch-once 语义)。
**验收**: `cargo test --workspace` 编译通过且该测试绿;`cargo clippy --workspace -- -D warnings` 通过。

---

## A · AgentRun 输入契约(拍平的根因)

### A1. `AgentRun` 公开 API 支持 system prompt + 多轮历史(P0)

**现状**: `AgentRun::start(config, input, ...)` 公开签名只接受单个 user turn 的 `RunInput`(`crates/orchest/src/run/mod.rs:45-59`);真正带 `initial_messages` 的 `start_with_bus` 是 `pub(crate)`(`run/mod.rs:62-69`)。使用方要传多轮历史只能把所有消息拍平成一条 user 消息(music-gift `agent.rs:210-212` 的 flat_map),wire 上 `system: ""`、角色结构全丢。
**建议**: 公开多轮启动入口(形如 `start_with_messages`,或把 initial_messages 提为 `AgentConfig`/`RunInput` 的一等字段),与 `resume_with_input` 的语义对齐;`AgentConfig.system_prompt` 保持现有行为。
**验收**: 使用方能以 `[System, User, Assistant, User…]` 完整角色结构启动 run,Anthropic Messages 协议 wire 上 `system` 字段非空、历史角色边界保留;music-gift 消除 flat_map。
**范围**: 小-中(签名公开 + 测试 + Py/Node 绑定透传)。

### A2. `AgentConfigBuilder` 暴露生成参数(P1)

**现状**: builder 无任何设置 `model.options`(RequestOptions:temperature/max_tokens/thinking 等)的方法;`max_tokens()` 只是预算上限(`run/config.rs:543-545`)。子代理场景(countdown 想要"低 temperature + 大输出预算")无法配置。
**建议**: builder 增加 `model_options(RequestOptions)` 或逐项 setter。
**验收**: 子代理可为单个 run 覆盖 temperature/max_tokens,不影响 model 默认值。

---

## B · Gen 协议与 run 终止语义

### B1. `GenRequest.params` 未知 key 不再静默丢弃(P0)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: `GenRequest.params: Value` 无 schema(`crates/orchest-protocol/src/capability.rs:64-68`);Suno `PASSTHROUGH_PARAMS` 白名单(`orchest-provider-http/src/gen/suno.rs:53-63`)之外的 key 静默丢弃。demo 传的 `genre/tempo/mood/vocal_style/instrumentation/production/exclude` 六个 key 全丢,`vocalGender`/`negativeTags`/`instrumental` 没人填,零报错——第三次 LLM 调用产出 100% 空转。
**建议**(两步走):
1. 小步: provider submit 时对不在 passthrough 白名单的 params key 打 `tracing::warn`(低基数,key 名进 span field 不进 metric label);
2. 大步: 音乐参数类型化(`MusicParams { negative_tags, vocal_gender, instrumental, style_weight, … }`),与 seam-findings Finding 2(`diagnostic_metadata` 垃圾袋、`GenAsset` 语义角色)合并设计。
**验收**: demo 当前提交的每个丢弃 key 都能在日志中看到 warn;类型化落地后,`vocalGender`/`instrumental`/`negativeTags` 有类型化入口,误拼 key 编译期或运行期显式报错。

### B2. MaxTokens 截断不得静默当成功(P0)

**现状**: `StopReason::MaxTokens` 且无 tool_use 时直接 `RunCompleted`,截断文本作为最终输出,无标记、无事件、无续写(`crates/orchest/src/run/actor.rs:966-977`)。countdown HTML 截断后以 `ready` 落盘(未闭合 `<script>` → widget 全废);review pass 截断丢 `<<<END>>>` 触发回退链。
**建议**: `RunCompleted` 事件(或 `RunResult`)携带 `stop_reason`/`truncated: true`;文档明确消费方应据此决定续写/重试/报错。
**验收**: 截断时消费方能从事件/结果中区分"完整完成"与"截断完成";现有 e2e 行为不变(仅信息增加)。

### B3. 异常 stop_reason + 无 tool_use 不再空转烧 token(P1)

**现状**: `ContextWindowExceeded` 等非 EndTurn/MaxTokens 且无 tool_use 时,流程落入工具阶段并 push 一条**空 content 的 User 消息**(`actor.rs:1538-1544`),step+1 后用相同上下文再次调用,直到 max_steps(默认 100)耗尽。
**建议**: 该分支直接 `RunFailed`(带 stop_reason),不 push 空消息。
**验收**: 构造异常 stop_reason 的 model stub,run 立即失败且 messages 中无空 User 消息。
**状态(2026-07-18)**: 已在 hotfix/2026_07_18b 修复(#216,PR #217,待合并)。
**后续(code review 记录)**: `StopSequence`/`Refusal` 这类"带内容的终止"目前也落入 RunFailed 桶(非回归——旧行为同样失败,只是更慢更贵);后续迭代可将这两个 variant 映射为完成(带截断/拒绝标记)而非 RunFailed。

---

## C · Provider 请求正确性(全使用方静默中招)

### C1. `cache_control` 挪到 block 级(P0,hotfix)

**现状**: `orchest-provider-http/src/messages.rs:250-254` 在默认 `CachePolicy::Auto` 下把 `cache_control` 写在请求**顶层**;Anthropic API 里它只能挂在 content block 上(`docs/external/anthropic/api.md:1435`)。结果:缓存从未生效(最好情况被静默忽略),严格端点可能 400——默认配置下**每个** Anthropic 请求都带这个非法字段。
**建议**: 按官方规范把 cache breakpoint 挂到 system/最后一条消息的 content block;保留 policy 开关。
**验收**: wire body 无顶层 `cache_control`;block 级 breakpoint 位置符合官方文档;现有 cache policy 单测更新通过。

### C2. thinking budget 与 max_tokens 默认组合非法(P0,hotfix)

**现状**: `RequestOptions` 默认 thinking=Medium(非 adaptive 模型映射 `budget_tokens=10240`,`messages.rs:235-244`)而适配器层 max_tokens 默认 4096(`defaults.rs:4`);Anthropic 要求 max_tokens > budget_tokens → 直接 400(claude-haiku-4 系等非 adaptive 模型)。
**建议**: 适配器发请求前校验并在 budget ≥ max_tokens 时联动调整(抬 max_tokens 或降 budget),并在 option_adjustments 里记录;或修正默认值使二者相容。
**验收**: 默认 options + 非 adaptive 模型不再产生 budget ≥ max_tokens 的非法请求;调整行为有 `option_adjustments` 记录。

### C3. catalog `context_window` 回填 ModelSpec(P1)

**现状**: catalog 每个模型都有 `context_window`(`protocol.rs:379`、`anthropic/profile.rs:136`),但从不回填到运行时 `ModelSpec.context_window_size`(默认 None,Py/Node 绑定硬编码 None)。后果:上下文硬校验(`actor.rs:709-735`)与 compaction(默认关)双双失效 → 上下文无限增长直到 provider 400、run 死亡。
**建议**: provider registry 构建 model 时把 catalog 的 context_window 回填 ModelSpec(用户显式值优先);compaction 保持 opt-in,但至少在有窗口大小时可用。
**验收**: 经 registry 创建的模型,`context_window_size` 非 None;超限前 compaction/硬校验按配置生效。

### C4. 模型调用重试与流中断(P1)

**现状**: 模型重试默认关(`run/config.rs:517` retry_policy 默认 None,Py/Node 绑定硬编码 None);SSE 流中断报 `stream_error`/`stream_interrupted`(`sse/mod.rs:300-302`),classify 为 NoRetry → 已流式输出的部分内容丢弃,run 直接 RunFailed。
**建议**: 流中断归类为可重试(在安全边界内:未交付 tool_use 前);考虑把"429/5xx/timeout/流中断自动重试 N 次"作为 builder 一行可开的默认推荐配置。
**验收**: 流中断的 stub 测试能重试成功;默认配置文档更新。

### C5. Chat 协议静默丢多模态 block(P1)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: `orchest-provider-http/src/chat.rs:137-149` 对 Image/Video/Audio block `_ => {}` 静默丢弃,无 adjustment、无警告。
**建议**: 丢弃时记录 `option_adjustments` 或发 warning 事件(遵循 observability.md "错误不得静默"原则)。
**验收**: 含图片的消息走 Chat 协议时,丢弃行为在 adjustments/日志中可见。

### C6. compaction 不得拆散 ToolUse/ToolResult 对(P1)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: `run/compaction.rs:77-79` 按条数硬切,切点落在 assistant ToolUse 与其 ToolResult 之间时,recent 窗口以孤儿 `tool_result` 开头 → Anthropic 400("unexpected tool_use_id")。
**建议**: 切分点对齐到 tool_use 块边界(必要时多保留/多裁一条)。
**验收**: 构造 tool_use 跨切点的用例,compaction 后消息序列无孤儿 tool_result。

### C7. 工具结果回插 role 一致化(P2)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: 串行路径以 `Role::User` 包 ToolResult(`actor.rs:1538-1544`),并行路径以 `Role::Tool`(`actor.rs:1004-1007`);快照/钩子语义不统一。
**建议**: 统一为协议层规范 role(以 Anthropic tool_result 惯例为准),迁移说明写入 changelog。
**验收**: 两条路径回插 role 一致;现有测试更新。

### C8. openrouter 测试环境变量竞态(P2)

**现状**(PR #218 评审附带发现): `providers::openrouter::tests::sends_custom_headers` 与 `resolve_headers_reads_env_values` 裸改 `OPENROUTER_APP_TITLE`/`OPENROUTER_SITE_URL` 环境变量,未走 `tests.rs` 的 `ENV_LOCK`——存在并行测试竞态 flake 面(评审中曾观测到一次失败,后续 16 次复跑未复现)。
**建议**: 两个测试改用 `ENV_LOCK` 串行化(与 tests.rs 其他 env 测试一致)。
**验收**: 高频复跑不再出现 env 相关 flake。

### C9. `js/index.d.ts` `runSync` 返回类型失真(P2,存量)

**现状**(v0.13 评审附带发现,base 上即存在): `js/index.d.ts` 声明 `runSync(...)` 返回 `RuntimeEvent[]`,但 napi 侧是 async fn、`js/index.js` 直接透传 addon,运行时返回的是 Promise。
**建议**: 核对 napi 真实行为,声明改为 `Promise<RuntimeEvent[]>`(或改实现真同步);顺带检查相邻声明是否同类失真。
**验收**: 类型声明与运行时行为一致;sdk-typescript 指南不误导。

---

## D · Skill 机制(skill 化推广的前提)

> demo 的 review.md / music_prompt/{provider}.md / countdown.md 本质都是知识型 prompt,适合 skill 化;
> 但以当前机制 skill 化只会让质量更差(无披露 + 加载无强制 + 失败静默)。本主题是推广 skill 的前置。

### D1. 渐进式披露落地为零配置一等抽象(P0,原则级)

**原则**: `docs/polaris/overview.md:17` 已把"渐进式披露作为一等抽象"列为 Skill-first 核心;2026-07-18 确认设计目标为**非常易用**——使用方指定 `skills_dir` 后披露链路全自动,"应用手写路径 + 模型自觉 read_file"这类脆弱用法不应再是必要手段。
**现状**: 运行时从不把 skill 的 name/description 注入 system prompt,也没有任何 skill 列表通道(`run/skills.rs:17-97`);纯知识 skill 完全靠应用手写路径 + 模型自觉 `read_file`,与 polaris 原则存在落差。
**设计**(对齐 Anthropic Agent Skills 三级披露):
1. **Level 1 元数据常驻**: 扫描 `skills_dir` 后,runtime 自动将全部 skill 的 name+description 以固定格式块(如 `<available_skills>`)注入 system prompt 尾部(无 system prompt 时自成一条);默认开启,`skill_disclosure: Off` 可关。单条元数据控制在 ~100 token(与 D4 的 description ≤1024 校验联动)。
2. **Level 2 正文按需加载**: 内置 `load_skill` 工具(name 参数),返回该 skill 的 SKILL.md 正文 + bundled 文件清单;路径由 runtime 按扫描结果解析,**消除 CWD 依赖**;命中即发 `SkillContentRead` 遥测。应用不再需要预注册 read_file 路径。
3. **Level 3 资源按需**: `load_skill` 支持可选 path 参数加载 SKILL.md 引用的 bundled 文件;canonicalize + 前缀检查防逃逸(复用 `bundled_tool.rs:101-108` 的做法)。

**易用性验收口径(关键)**: 全新使用方只做两件事——放好 SKILL.md 目录、设 `skills_dir`——模型即可知晓可用 skill 列表、在合适时机调用 `load_skill` 并遵循其方法论;全程无需手写任何路径、无需注册任何工具、无 CWD 依赖。用 music-gift 的 lyrics-writer 做验收用例:删掉 system.md 手写路径与预注册 read_file 后,生成质量不回归。
**范围**: 中(registry 注入点 + `load_skill` 工具 + 遥测 + Py/Node 绑定透传配置)。

### D2. scanner 错误上报 + frontmatter 解析加固(P0)

**现状**: `skill/scanner.rs:103-107` 对读取/YAML 错误一律 `.ok()?` 返回 None,skill 无声消失;`scan_recursive`(`scanner.rs:74`)对 read_dir 失败同样静默;frontmatter 用 `find("---")` 切分(`scanner.rs:149`),description 含 `---` 即提前截断。
**建议**: 解析失败发结构化 warning(RuntimeEvent 或 tracing::warn,带路径+原因);frontmatter 改为行级状态机(只认独立一行的 `---` 结束符)。
**验收**: 坏 frontmatter 的 skill 产生含路径与原因的警告;description 含 `---` 的合法 skill 解析正确。

### D3. 单个坏 skill 不拖垮整个 run(P1)

**现状**: `register_skills` 任一条目失败(脚本 canonicalize 失败、工具重名、白名单外)→ 整个 run `fail_pre_start` 发 `RunFailed`(`run/actor.rs:222-244`)。
**建议**: 跳过失败 skill + 结构化警告(默认);保留严格模式开关。
**验收**: 一个坏 skill 与一个正常 skill 并存时,run 正常启动,正常 skill 可用,坏 skill 有警告。

### D4. Anthropic Agent Skills 标准兼容(P1)

**现状**: 官方规范可选字段是 `allowed-tools`(连字符),scanner 只读 `allowed_tools`(下划线,无 serde alias,`scanner.rs:19`)→ 标准 skill 的字段被静默忽略;无 name 格式校验(kebab-case、1-64、与目录名一致)、无 description ≤1024 校验。
**建议**: 加 serde alias(两种写法都收);按官方规范补 name/description 校验(违反 → D2 的警告)。
**验收**: 官方示例 skill 的 `allowed-tools` 被正确解析;非法 name/description 产生明确警告。

### D5. 死字段/死变体:兑现或移除(P2)

**现状**: `SkillManifest.allowed_tools` 除测试外无消费方;`capabilities` 只 enforce env(network/filesystem_read/filesystem_write/max_memory_mb 解析后从不生效);`RuntimeEvent::SkillDependencyError` 从不发射。
**建议**: 按 v0.3 capabilities 声明的锁定设计决定:兑现 enforce(文档化语义)或在 spec 中显式标注为"声明预留,不 enforce";`SkillDependencyError` 接上 env 构建失败路径或移除变体。
**验收**: 每个字段/变体要么有行为+测试,要么文档显式标注预留。

### D6. skill name 路径注入防护(P1)

**现状**: `skill/env_manager.rs:50-56` 用未校验的 `manifest.name` 拼 `skill-envs/{name}-{hash}`;name 含 `/`/`..` 可塑造缓存目录路径(source 是用户提供的 SKILL.md)。
**建议**: name 校验(与 D4 的 kebab-case 校验合并)或路径组件转义;拼接后 canonicalize + 前缀检查。
**验收**: 恶意 name(`../x`)无法逃逸 skill-envs 目录。

### D7. 一致性小修(P2)

- 小写 `skill.md` 能被发现(`scanner.rs:93`)但 `run/skills.rs:58` 只登记大写 SKILL.md 的遥测 → 统一。
- skill 重名无检测(只有 bundled tool 名冲突才报错)→ 重名警告。
- 脚本 stderr 直接 `eprintln!` 进宿主进程(`bundled_tool.rs:288,341`)→ 进 tracing span。

---

## E · agent-as-tool 子代理语义

### E1. child run 失败应返回 Err 而非 Ok(Structured)(P1)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: `tool/agent_as_tool.rs:211-224` 把 child run 失败包成 `details{"error": …}` 照常返回 `Ok(Structured)`;消费方必须自己扒 details——demo `countdown.rs:56-58` 的注释("Matching only on Immediate made every single countdown fail here")就是踩坑记录。
**建议**: child run `RunFailed` → tool 返回 `Err(ToolError)`(kind 可区分 Transient/Fatal,复用重试语义);保留 details 里的诊断。
**验收**: child 失败时消费方拿到 Err;countdown 类消费方无需匹配 details["error"]。

### E2. 子代理输出契约(提取加固)(P2)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: 子代理只有纯文本 output,消费方靠 `strip_code_fences` 这类脆弱逻辑提取(countdown `strip_code_fences` 只认严格首尾 fence;模型前后加解释文字或输出完整 `<!DOCTYPE>` 都会原样落盘)。
**建议**: 评估为 agent-as-tool 增加输出格式契约(如 `expect: { fenced: "html" }` 或 JSON schema 模式),由 SDK 侧做提取/校验/重试提示。
**验收**: 格式不符的输出在 SDK 层被拒并给模型纠正提示,而不是原样交给消费方。

---

## F · trajectory 录制(决策点,P2)

**现状**: 三通道可观测性(RuntimeEvent/tracing/metrics)已按 `docs/polaris/observability.md` 落地;但"事件级 trajectory 落盘/回放"从未规划——文档立场是留给上层应用,隐私红线默认禁止 prompt/tool 参数/生成文本进日志。后果:music-gift 的拍平 bug 能存活至今,正是因为没有任何地方记录"agent 实际收到的消息序列"。
**决策点**: 是否提供 SDK 官方的 dev-only trajectory recorder(如 `orchest` 内一个 feature-gated 的 event→JSONL subscriber,显式标注含敏感 payload、仅限本地调试)?还是坚守 polaris 立场只给 demo/应用侧示例?
**建议**: 倾向后者(polaris 不动)——在 examples 或 guide 里给一个 recorder 参考实现,README 写明数据风险;若未来 v1.0 用户反复要求,再评估进 SDK。
**验收**: 决策结论写入 polaris/observability.md 或本文档;若做参考实现,本地跑 run 能得到完整事件序列 JSONL。

---

## G · Tool 调用易用性(seam-findings Findings 3-5)

> 来源: music-gift 架构评审(commit `2f4f880`)记录的三处 SDK 缺口,详见
> [`examples/demo/music-gift/docs/seam-findings.md`](../../examples/demo/music-gift/docs/seam-findings.md)。

### G1. `ToolContext` 一次性调用 helper(P1)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: 在 run 之外"调一次工具"必须手工伪造整个 `ToolContext`(`RunId::new()`、`ApprovalBus::default()`、`run_depth: 0`、编的 `tool_call_id`、`event_tx: None`、默认预算、空 `parent_messages`)——demo countdown 子代理调用就是这么做的(`countdown.rs:153-163`)。这些字段对一次性调用全部无意义,但全部被迫构造。
**建议**: 提供 `ToolContext::oneshot()`(或 `Tool::call(input)` 便捷方法,内部构造平凡 context)。
**验收**: 一次性工具调用零样板;调用方无需知道哪些字段可以安全伪造。

### G2. 测试中的 `ToolContext` 构造(P1,与 G1 同解)
**状态(2026-07-26)**: 已在 v0.15 落地。

**现状**: 因为没有便捷的 `ToolContext` 构造方式,demo 的 `collect_info` 测试(`collect_info.rs:108`)放弃执行工具本身,改为**把校验逻辑复制进测试模块测副本**——两份实现可静默漂移,工具真实逻辑变了测试照样绿。
**建议**: G1 的 helper 同时解决测试场景;任何"在 run 外执行工具"的支持路径都应让测试直接走真实代码路径。
**验收**: demo 测试改为执行真实 tool callback,复制的逻辑删除。

### G3. `ChatModel` / `ModelAdapter` 双名收敛(P2)

**现状**: `ModelAdapter` 是 `ChatModel` 的别名,但调用点不透明:`AgentRun::start` 收 `Arc<dyn ModelAdapter>`,持有 `Arc<dyn ChatModel>` 的调用方必须显式 `as` cast(demo `agent.rs:216`)。需要 cast 的别名不是别名。
**建议**: 二选一——runtime 直接接受 `Arc<dyn ChatModel>`(或泛型 `Into`),或把公开 API 收敛为单一规范名(v1.0 前处理,避免公开 API 长期双名)。
**验收**: 调用点零 cast;公开 API 只出现一个规范名。

---

## 依赖与建议落地顺序

1. ~~Hotfix~~ ✅ hotfix/2026_07_18b(PR #217)。
2. ~~质量地基迭代~~ ✅ v0.13(PR #223): A1 输入契约、B2 截断标记、C3 上下文回填、C4 重试。
3. ~~Skill 机制迭代~~ ✅ v0.14(PR #230): D2 scanner、D1 零配置披露、D3/D4/D6、D5/D7。
4. ~~Gen 协议迭代~~ ✅ v0.15: B1(类型化 MusicParams + 未知 key 警告,与 Finding 2 合并设计;GenAsset 角色 + duration 类型化)→ E1(child 失败 Err)/E2(输出契约)→ C5-C7;G1/G2(oneshot helper)并入本迭代。
5. **Tool 易用性**: ~~G1/G2~~ ✅ 并入 v0.15;G3(双名收敛)留 v1.0 前。**绑定跟进(v1.0 冻结前)**: v0.15 的 `expect_output` 契约尚未暴露到 Py/Node 绑定(`register_agent_tool` 只透传 input/output mapper)——绑定用户拿到 E1 的 Err 语义但用不了 E2 的提取,与 G3 一并裁定。
6. **F 决策** 可在任意时间点做,不阻塞其他项。

demo 侧对应依赖见 [`examples/demo/music-gift/docs/optimization-plan.md`](../../examples/demo/music-gift/docs/optimization-plan.md)。
