# music-gift 优化计划(生成质量 + countdown widget)

> 状态: **待执行** | 记录于 2026-07-18
> 来源: guided pipeline 全链路实测梳理,所有问题均经代码核实并附文件:行号。
> SDK 侧配套计划见 [`docs/todo/2026-07-18-sdk-optimization-plan.md`](../../../docs/todo/2026-07-18-sdk-optimization-plan.md)(文中以 SDK-A1、SDK-B2 等引用)。
> 本 demo 的定位是暴露 SDK 缺口 —— 纯 demo 可修的立即修;依赖 SDK 缺口的标注依赖,不绕过。
>
> **核验记录(2026-07-18)**: 架构评审重构 `2f4f880` 后逐项复核——D1-D16 **全部仍然成立**(重构忠实、无 wire 变更),
> 行号已全部更新为重构后位置;`GenSubmission` 类型化未顺带修 D1(仅 lyrics/style/title 三字段);
> 重构新引入两个小问题记为 D17。
>
> **评审记录(2026-07-18,PR #218 code review)**: 合并前评审修复 1 Critical(smoke 测试失效)+ 15 Important
> (后端:generate 鉴权+幂等、auth 可达 panic、body limit 死代码、SQLite busy_timeout、stream 超时收尾、prompt 模板 CWD 依赖;
> 前端:IME Enter 误提交、宠物 transcript 伪造、music 步恢复死胡同、自定义场景死按钮、空歌词兜底、Playlist 错误分层、LRC 滚动自抑制、LRC 行高破版、i18n 补全 68 key×5)。
> 两路评审的剩余 Minor 收录为 D18(后端)/D19(前端)。

## 背景:质量问题的三个根因

1. **精心构造的内容没送到**:EnrichedPrompt 八维度在 Suno 链路 100% 空转;system prompt + 多轮历史被拍平成单条 user 消息(wire 上 `system: ""`)。
2. **失败全部静默**:review 失败、music prompt 改写失败、JSON 解析失败、校验不通过——全部静默回退,用户拿到"无审核歌词 + 裸 style"直通 Suno,运维无感。
3. **countdown 单发无校验**:无重试、无续写、无完整性检查,prompt 约束粒度不足,质量上限被锁死。

优先级定义: **P0** = 纯 demo 可修、直接拉低音乐/倒计时质量;**P1** = 依赖 SDK 项落地或显著提升质量上限;**P2** = 打磨与文档纠偏。

---

## P0(立即修,不依赖 SDK)

### D1. Suno 提交参数对齐:英文 style + vocalGender + instrumental + negativeTags

**问题**:
- `style` 传的是前端 i18n 中文标签("治愈温暖"四个汉字)直进 Suno `style` 字段(`frontend/src/i18n.tsx:380-390` STYLE_TAGS → `GuidedFlow.tsx:364` → ReviewCard → `useMusicGen.start` → meta.style → `src/tools/music_gen.rs:164-175` 进 params.style);
- 用户选的人声性别只进了被丢弃的 `vocal_style`,Suno 的 `vocalGender` passthrough 存在(`crates/orchest-provider-http/src/gen/suno.rs:57`)却没人填;
- `exclude` key 名与 Suno 的 `negativeTags` 不匹配,静默丢弃;
- 重构后的类型化 `GenSubmission`(`music_gen.rs:85-89`)只有 `lyrics/style/title` 三字段,**连 kind 都没有**——instrumental 判断(`music_gen.rs:120`)不进 submit,纯音乐礼物仍收到 `instrumental:false`;
- params 的 7 个 key(`genre/tempo/mood/vocal_style/instrumentation/production/exclude`,`music_gen.rs:164-175`)全部不在 `suno.rs:53-63` 白名单,静默丢弃。
**修法**:
- i18n 风格标签加英文映射表(label 给用户看,英文 tag 给 provider);
- `GenSubmission` 扩字段(vocal/kind 等),`vocal` → `vocalGender`("male"/"female"),`exclude` → `negativeTags`,kind="instrumental" → `instrumental: true`;
- 提交前构造 params 时对齐 `suno.rs:53-63` 的 PASSTHROUGH 白名单逐 key 核对。
**验收**: Suno 请求体中 `style` 为英文、`vocalGender` 与前端选择一致、`negativeTags` 生效;instrumental 礼物收到无人声结果;其余 provider(mureka/minimax)路径不回归。
**注**: SDK-B1 落地后(未知 key warning)可再加一层防回归。

### D2. 静默失败链全部加日志 + 降级可见

**问题**:
- music prompt 改写失败 `.unwrap_or_else` 连错误日志都不打(`music_gen.rs:141`;polish 端点同款 `routes.rs:483`);
- JSON 解析失败静默回退 `EnrichedPrompt::fallback(style)` = `"{style}, high quality music production"`(`music_gen.rs:529`);
- review pass 失败/空输出静默回退原始文本,只 `eprintln`(`src/agent.rs:281/299/321/329`);
- `lyrics_validator` 全部检查 + `check_style_prompt` 只 `eprintln` 不阻断(`music_gen.rs:113-115, 144-146`)——且 `check_style_prompt` 检查的是根本不会发给 Suno 的 enriched.prompt,真正上线的 submission.style 从不被检查;
- `Done` 事件无 `degraded` 字段(`agent.rs:165-173`)。
**修法**: 统一改 `tracing::warn!`(带 gift_id / 阶段 / 原因);`Done` 事件或 gift 记录里加 `degraded: ["review", "music_prompt", …]` 标记,前端可提示"本次生成跳过了审核";`check_style_prompt` 改为检查实际提交的 style 字段。
**验收**: 任一环节失败,server.log 有结构化 warn 且前端能感知降级;fallback 发生时礼物页可见提示。

### D3. countdown 日期语义修复(时区 + 闰年)

**问题**: prompt 要求 `new Date('{target_date}')`(`prompts/countdown.md:42`),`new Date('2026-01-15')` 按 **UTC 零点**解析——中国时区倒计时**提前 8 小时归零**(样例 `data/countdown/4594127a41aa.html:210` 即为实例);`parse_birthday_info` 对今年已过的生日直接 `+365`(`src/tools/countdown.rs:138-140`),闰年/2 月 29 日出错。
**修法**: prompt 改为生成本地零点构造(`new Date(y, m-1, d)` 或显式 local midnight);`parse_birthday_info` 用日历加法算"下一个该月日",2/29 按 2/28 或 3/1 文档化处理。
**验收**: 东八区环境生成的倒计时归零时刻为当地生日 00:00;2/29 生日的用例有明确行为+测试。

### D4. countdown prompt 加固 + 生成完整性校验 + 带错重试

**问题**:
- prompt 无响应式要求、无 CJK 字体指导(样例 Baloo 2 只有拉丁字形,中文名全落回系统字体)、无复杂度预算(与 4096 token 上限叠加必截断)、未禁 emoji(样例糖果雨 🍬🍭 与 SVG 线稿风格冲突)、无输出完整性自检;
- 截断的 HTML 会以 `ready` 落盘,未闭合 `<script>` → JS 全废,倒计时定格;`run_countdown` 只查空串(`countdown.rs:67-69`);
- `{previous_error}` 占位符(`prompts/countdown.md:46`;`countdown.rs:114` 替换为空串)说明规划过带错重试但**未实现**。
**修法**:
- prompt 增加:输出预算(如"单个 SVG 图标 ≤ 3 个,总长度克制")、CJK 字体栈(系统中文字体 fallback)、禁 emoji 一律 SVG、响应式(clamp/flex-wrap 底线)、**必须以 `</script>` 结束**;
- 后端校验:提取的 html 以 `</script>` 结尾且非空,否则视为失败 → 把错误原因填入 `{previous_error}` 重试一次(实现已预留的机制),再失败置 `failed`;
- 依赖 SDK-B2(截断标记)落地后可改为按 stop_reason 判定,先以结尾校验兜底。
**验收**: 截断输出不再落 `ready`;带错重试路径有测试;新样例 HTML 无 emoji、中文渲染不回落系统默认、窄屏不破版。

### D5. countdown 安全 + 生命周期兜底

**问题**:
- `/api/countdown-section/{id}` 裸 serve `text/html` 且无 CSP/sandbox 响应头(`routes.rs:611-624`):直接打开 URL 时内联脚本在应用源执行,可携带 HttpOnly cookie 代发请求;name/lyrics 用户输入直进 prompt,prompt injection 攻击面真实存在(iframe 路径已安全,`CountdownFrame.tsx` `sandbox="allow-scripts"` 正确);
- 前端轮询死循环:`failed` 后服务器返回 404,`res.ok` 为 false 不清 interval,永远轮询(`GiftPage.tsx:186-200`);初始即 failed 则完全静默(`loadCountdown` 只认 ready/pending);countdown 轮询 interval 在组件卸载/id 切换时无 cleanup(cdPollRef 不在 effect 清理);
- `std::fs::write` 非原子(`countdown.rs:75`),崩溃留半截文件。
- 注:重构的 watchGeneration 修的是**音乐生成** SSE 的 timeout 卡死,与本项精神同向但不是本条目;countdown 逻辑重构未碰。
**修法**: 路由加 `Content-Security-Policy: sandbox` 或 `default-src 'none'` 类响应头;前端非 200 清 interval + 轮询总时长上限 + effect cleanup + failed 态 UI 占位;写入改临时文件 + rename。
**验收**: 直接打开 countdown URL 时脚本不执行(DevTools 验证);failed 后网络面板无持续轮询;kill -9 中途不产生半截 html。

---

## P1(质量上限;部分依赖 SDK)

### D6. 取消拍平:system prompt + 多轮历史各归其位 【依赖 SDK-A1】

**问题**: `run_chat_agent` 把 system 文本+照片+全部历史 flat_map 成单条 user 消息(`src/agent.rs:210-212`),`AgentConfig` 从不调 `.system_prompt()`(`agent.rs:184-191`),wire 上 `system: ""`、多轮历史无角色标记;review pass 同样拍平(`agent.rs:295-296`);`build_messages` 静默丢弃 incoming system 消息(`src/agent/message.rs:100`,FreeCreatePanel 自由模式指令从未生效)。SDK-A1 落地前这是 SDK 逼出来的变通。
**修法**: SDK-A1 后,`.system_prompt(build_system_message(meta))` + initial_messages 传完整角色结构;review pass 同理;`message.rs:100` 对 incoming system 显式拼接或报错,不再静默丢。
**验收**: Anthropic 请求 wire 上 `system` 非空、历史 user/assistant 角色边界完整;自由模式 system 指令生效。
**同步修**: `docs/guided-pipeline.md:108` 称 review pass "用 review.md 做系统 prompt"——实现与文档一致后再核对文档。

### D7. SKILL.md 加载强制化 【SDK-D1 落地前用临时方案】

**问题**: 写词方法论是否生效全靠模型自觉 `read_file`(system.md:65-68 指示 + `agent.rs:198-208` 注册);拍平削弱指令权重,模型不调则整套方法论不进上下文;SDK 按 CWD 相对路径读(`crates/orchest/src/tool/builtin.rs:139-141`),CWD 不对直接 READ_ERROR 白烧 max_steps(仅 5 步)预算。注:`main.rs:40` 的 skills_dir 本是绝对路径,路径半项天然满足。
**修法(临时)**: 后端直接把 SKILL.md 内容拼进 system prompt(牺牲渐进披露换可靠性),skill 文件改纯数据源。
**修法(正式)**: SDK-D1(零配置渐进式披露)落地后移除临时拼接——模型经 Level 1 元数据知晓 lyrics-writer、自主 `load_skill` 加载正文;同时删掉 system.md 手写路径与预注册 read_file,作为 SDK-D1 易用性验收口径的实证用例。
**验收**: 连续 N 次生成,SKILL.md 内容每次都在上下文中(以日志/事件验证),不依赖模型自觉。

### D8. collect_info schema 与 system.md 冲突消解

**问题**: 工具强制 `name/scene/emotion_direction` 三必填(`src/tools/collect_info.rs:32`),system.md 却说"信息够就直接生成、最多问 2 个问题"(`system.md:26/52-55`)——模型要么为凑字段编造,要么纠结不调。
**修法**: 字段改可选(由 prompt 约束收集纪律),或 system.md 明确"调 collect_info 时机"。
**验收**: 信息齐全时模型直接生成不纠结;信息不足时追问且不乱编字段。

### D9. 元标签输出格式统一

**问题**: SKILL.md 把 `<<<STYLE>>>/<<<TITLE>>>/<<<VOCAL>>>` 放在 `<<<LYRICS>>>` 块**内部**(SKILL.md:180-193),review.md 放在 `<<<END>>>` **之后**(review.md:60-65);`agent.rs:71-77` 注释自认两套打架,靠 `strip_meta_tags` 兜底——review pass 回退时解析路径完全不同。
**修法**: 两处 prompt 统一为一种格式(建议 END 之后),解析只留一条主路径 + 明确错误分支。
**验收**: 同一歌词经"有/无 review pass"两条路径解析结果一致;`strip_meta_tags` 兜底删除或仅做防御。

### D10. review.md 中文歌词适配

**问题**: 10 点清单里发音表全是英文 homograph(`review.md:19-24`,live/read/lead…),phonetic 拼写规则对中文歌词无意义,但 reviewer 被命令 "AUTO-FIX … No exceptions",可能为改而改、误伤中文歌词;错拼修正(liv/lyve)会原样进 Suno 歌词框。
**修法**: review.md 按歌词语言分节,英文规则显式标注"仅英文歌词适用";中文歌词只审结构/押韵/字数类项。
**验收**: 中文歌词经 review pass 后无非预期改写(对比输入输出 diff 只动该动的项)。

### D11. 校验门:关键项从 warning 升级为阻断 + 重试

**问题**: `lyrics_validator` 所有检查(结构标签、chorus≥2、词数 100-600、双生 verse、艺人名块单)只 warning(`music_gen.rs:112-115`);全链路唯一硬拒绝是空歌词(`music_gen.rs:120-124`);对照组 bitwize-music 的 pre-generation-check 是 fail 阻断,本 demo 没有任何 gate 会阻断——`docs/quality-gaps.md:55` "cover 5 of 6 gates" 的叙述与实现不符。
**修法**: 关键项(无结构标签、词数越界、艺人名命中)失败 → 不提交 Suno,带问题清单回 review pass 重试一轮,再失败向前端报错。
**验收**: 构造坏歌词,生成被阻断且前端可见原因;质量 gate 语义与 quality-gaps.md 叙述对齐(同步改文档)。

### D12. countdown 生成参数独立配置 【依赖 SDK-A2】

**问题**: countdown 子代理由 `build_model_or_default(COUNTDOWN_MODEL_ENV)` 构建(`config.rs:179` → `build_countdown_tool` `.model(model)` `config.rs:160`),模型取 `MUSIC_GIFT_COUNTDOWN_MODEL` 否则回落 chat model;max_tokens 仍只读 `MUSIC_GIFT_CHAT_MAX_TOKENS`(`config.rs:89-91`),无专用变量;temperature 等无法配置(SDK-A2 缺口)。注:重构删除的只是从未被读的 `AppConfig/AppState.countdown_model` 结构字段,不影响本项前提。
**修法**: 加 `MUSIC_GIFT_COUNTDOWN_MAX_TOKENS`(默认调大,如 8192);SDK-A2 后 countdown 子代理显式设低 temperature。
**验收**: 环境变量可独立控制 countdown 输出预算;temperature 覆盖生效。

---

## P2(打磨 + 文档纠偏)

### D13. countdown 模板双重注入清理

**问题**: 原始模板(含字面 `{name}` 占位符)作 system prompt(`src/config.rs:147`),替换版又作 user message(`config.rs:163-167` + `countdown.rs:106-115`)——模板进两次,浪费 token 且占位符原文干扰模型。
**修法**: system prompt 用固定角色描述,替换后模板只作 user message。
**验收**: 每次 countdown 调用 prompt token 数下降,输出质量不回归。

### D14. countdown 可观测性补齐

**问题**: `tool_context()` 里 `event_tx: None`(`countdown.rs:158`,函数体 153-164)零事件;失败仅 `eprintln`(`routes.rs:275`)。
**修法**: 接上事件通道(生成开始/完成/失败),失败进结构化日志;SDK-G1(`ToolContext::oneshot()`)落地后可顺带去掉手工伪造 context 的样板。
**验收**: countdown 生成全过程在 server.log 可追溯。

### D15. 文档纠偏(guided-pipeline.md / quality-gaps.md)

**问题**(与实现的偏差,修复后以代码为准回写):
- guided-pipeline.md 经重构更新后仍全文未提拍平;`:108` 仍称 review pass"用 review.md 做系统 prompt"(实现是拍平成单条 user 消息);
- guided-pipeline.md Step 8(`:171-185`)仍展示 EnrichedPrompt 八维输出进 submit、未注明 Suno 默认链路全丢(D1 修后恢复成立);
- quality-gaps.md 完全未动:"Closed" 表(`:11-17`)多处不成立——Exclude Styles / Artist names 检查 / pronunciation 强制 / suno.md 八维度,产物都不上线;真正 closed 的只有歌词文本内 performance cues;
- quality-gaps.md `:55` "cover 5 of 6 gates" vs 实际无任何阻断 gate(D11)。
**修法**: D1/D6/D11 落地后逐项回写;修前先在两份文档加"当前实现偏差"警示段,避免误导。
**验收**: 文档描述与代码行为逐条一致。

### D16. trajectory 录制(复盘质量问题的手段)

**问题**: 没有任何地方记录"agent 实际收到的消息序列/每次 LLM 调用的输入输出"——拍平 bug 能存活至今正是因为无法复盘。SDK 侧立场见 SDK-F(polaris 把完整轨迹留给应用)。
**修法**: demo 侧加 dev-only 事件 subscriber:订阅 RuntimeEvent + 每次 LLM 调用的请求/响应,落 `data/trajectory/{run_id}.jsonl`,README 写明含敏感数据仅限本地。
**验收**: 本地跑一次 guided 流程,能从 JSONL 复盘每个阶段的模型输入(wire 级)。
**注**: 这与 D2(失败日志)互补——D2 管"哪环挂了",D16 管"当时给模型看了什么"。

### D17. 重构遗留小问题(2f4f880 核验发现)

**问题**:
- `GiftMeta::from_value` 全有或全无(`src/gift.rs:89-91`):meta 反序列化失败 → `unwrap_or_default()` 全部字段回落默认;旧的逐 key 读取只丢坏的那个 key——一个畸形 key(如 `style: 123`)会把 name/birthday 等好 key 一起吞掉,`create_gift` 的 birthday 判定(`routes.rs:205`)同受影响。实际风险低(需前端发畸形 meta),但与注释声称的 "matching the old per-key reads" 不符;
- `poll()` 的 `unreachable!()`(`music_gen.rs:251`)依赖上方 `if status == Done` 块两个分支都 return——当前成立,给未来改动留地雷。
**修法**: `GiftMeta` 改逐字段容错(serde `#[serde(default)]` per-field 或手动逐 key);`poll()` 改 exhaustive match 消掉 `unreachable!()`。
**验收**: 畸形单 key 不影响其余 key 解析(新增单测);`poll()` 无 `unreachable!()`。

### D18. 后端健壮性 Minor 批(PR #218 评审收录)

**来源**: 2026-07-18 PR #218 code review 后端 chunk 的 Minor 清单(当轮 Critical/Important 已修)。行号以评审时为准,可能已漂移。
- `agent.rs` + `routes.rs`:RunFailed 时客户端收到两个 Error 事件(真实错误 + 泛化 "agent run failed"),留一条且不丢原始信息
- `routes.rs` like_gift:viewer_id 客户端自报、无长度上限、无 unlike 路径;likes 数组随 GET 公开——点赞可注水
- `music_gen.rs` poll():只对 done 短路;failed 的 gift 会拿旧 handle 再 poll provider,行为依赖 provider
- `countdown.rs`:birthday 解析失败回落硬编码 `"2025-01-01"`(已过去);建议 create_gift 校验 `M-D` 格式,失败不触发 countdown
- `lyrics_validator.rs`:子串匹配误报(`queen` 常用词、`enya ⊂ kenya`);`word_count` 按 whitespace 切分对中文无意义——**D11 升级阻断前必须先修**,否则误杀
- `auth.rs`:OAuth `state` 硬编码不校验(登录 CSRF);send-link 无限流;过期 sessions/magic_tokens 永不清理
- `error.rs`:500 响应把内部错误串(DB 错误、文件路径)回给客户端;SSE Error 事件同样——随 D2 落地统一收
- `routes.rs` verify_creator 非常量时间比较 token(demo 低风险);gift id 只取 UUID 前 12 hex(48 bit,建议 ≥16)
- 照片生命周期:delete_gift 不清 `data/photos/` 孤儿文件;`/photos/{name}` 无路由 serve(需确认前端是否破图);`ChatRequest.photos` 数量无上限(逐个读盘+base64 进 LLM 上下文,成本放大道)
- `Cargo.toml`:tower-http `cors` feature 启用但无 CorsLayer——删 feature 或补 layer
- 两份 `unix_now`(routes.rs / auth.rs);`ChatRequest.lang` 接收后弃置(删字段或真用)

### D19. 前端打磨 Minor 批(PR #218 评审收录)

**来源**: 2026-07-18 PR #218 code review 前端 chunk 的 Minor 清单(当轮 Important 已修)。
- `useMusicGen.watchStream` 无 unmount cleanup;`GuidedFlow.startChat` fetch/rAF 无 AbortSignal——中途切页对卸载组件 setState
- sessionStorage 恢复只 catch JSON 语法错误,无形状校验(旧版本漂移 → 渲染期 crash);建议加 version 字段
- 切语言即丢快照(lang 不匹配丢弃);已存气泡 label 保留旧语言混排
- 键盘可访问性系统性缺口:`role="button"` 无 onKeyDown(Enter/Space 不触发)多处——换原生 `<button>` 或补 key handler
- 场景 pill 双击触发两条后端聊天流(双倍 LLM 调用):PillsRow 选中后禁用或 startChat 挡重入
- `GiftPage.handleShare` 无 clipboard 降级(非安全上下文 undefined 同步抛);`liked` 不持久(刷新消失);`AudioPlayer.toggle` play() reject 无 catch
- polish:响应未校验(`d: any`,prompt 缺失时受控变非受控);`provider:"suno"` 硬编码(后端换 mureka/minimax 时模板错配,与 D1 同源)
- `ReviewCard` 标签连接符 `、` 与语言无关;`GiftPage` LRC seek 用 `document.querySelector("audio")`(脆弱,改转发 ref);review 审核报告不持久化(刷新即丢)
- `LoginModal` 无 Esc 关闭/焦点管理;`useAuth` logout fetch 失败 unhandled rejection
- `GuidedFlow` 渲染体内 setTimeout(StrictMode 双发);mount-only effect 缺依赖(补注释)
- `watchGeneration` onDone 边界:audio_url 为 null 时状态自相矛盾(置 ready 但渲染回落 "Generate Music")
- PlaylistPage 多 AudioPlayer 无播放互斥
- i18n 清单外残留硬编码:GuidedFlow aria-label、GiftPage "Untitled"/"for {name}"、PlaylistPage "Untitled"、UnwrapStage aria、FreeCreatePanel title/aria

---

## 依赖关系与建议执行顺序

```
P0(本周可做完):
  D1 参数对齐 ─┐
  D2 失败日志  ├─ 直接提升音乐质量下限,不依赖 SDK
  D3 日期修复  │
  D4 countdown prompt+校验+重试
  D5 countdown 安全/轮询/原子写

P1(SDK hotfix/迭代落地后跟进):
  SDK-A1 ──→ D6 去拍平(歌词质量的最大单因)
  SDK-D1 ──→ D7 skill 加载正式化(之前用临时拼接)
  SDK-A2 ──→ D12 countdown 参数独立
  SDK-B2 ──→ D4 截断判定改 stop_reason(先有结尾校验兜底)
  D8-D11 不依赖 SDK,可与 P0 并行

P2(随手做):D13 D14 D15 D16 D17 D18 D19
```

**验收总口径**: P0 + D6 完成后,跑一次端到端 guided 流程(中文、女声、生日场景),确认:① Suno 请求体含英文 style/vocalGender/negativeTags;② wire 上 system 非空、历史角色完整;③ 任一环节注入故障,日志与前端均可见;④ countdown 归零时刻为本地生日零点,HTML 无 emoji/响应式不破版。
