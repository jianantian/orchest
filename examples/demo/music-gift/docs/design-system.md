# Moment 设计系统（v1 · 已定稿 2026-08-08）

> 状态：**已定稿**。本文档是 music-gift 前端唯一的设计契约；所有视觉/交互改动先改本文档，再改代码。
> 定稿结论：视觉方向 = **A「暖纸·金光」**（用户经原型对比选定，原型稿 `/prototype/design` 已随定稿删除）；其余决策点（②–⑦）按 §9 推荐默认值生效。
> 方法来源：Apple 流体界面设计原则（WWDC *Designing Fluid Interfaces* / *Principles of Great Design*），翻译为 Web 平台（React + Motion）。
> 已定前提（2026-08-08 与用户确认）：范围 = 完整设计系统；品牌方向允许重新探索；动效引入 Motion 弹簧库；本文档即落点。

---

## 1. 设计原则（产品级）

Moment 是"为某人做一首歌"的情感产品。界面要服务的不是效率，而是**心意被看见**。四条原则，按优先级：

1. **仪式高于效率**。拆礼物、等生成、第一次播放是产品的三个仪式时刻，允许它们"慢"，但必须慢得连续、可预期、可中断。
2. **响应是地基**。一切可触摸的东西在 pointer-down 瞬间给出反馈；反馈在交互全程连续，而不是只在结束时播一段动画。
3. **动效可中断**。任何动画在任何时刻可以被反向、被接管：从屏幕上的当前值起步，继承速度，不锁输入。这是引入 Motion 的唯一理由——CSS keyframes 做不到。
4. **克制即品牌**。一种强调色、一套圆角、一组弹簧预设。任何新值必须能归入既有 token，否则先改本文档。

对照 Apple 八原则裁剪后的适用项：Purpose（不做与"送礼"无关的功能）、Agency（生成可中断、草稿可撤销、删除两段确认）、Familiarity（things that look the same behave the same——pill 永远意味着单选）、Craft（每个时长/缓动值可辩护）、Delight（只保留三个仪式时刻的惊喜，其余全部安静）。

---

## 2. 视觉方向

### 2.1 推荐方向 A：「暖纸 · 金光」（精修现行基线）

延续奶油底 + 暖金 + 衬线标题的情感定位（它是对的：礼物 = 纸张、丝带、手写体），做三件事把它从"风格"升级为"系统"：

- **材质分层**。全站只有三种材质：纸面（paper，内容与卡片）、织物（fabric，礼盒/仪式时刻的丝绒面）、玻璃（glass，header 等悬浮层，半透明 + backdrop-blur）。材质即层级：玻璃浮于纸面之上，织物只出现在仪式时刻。
- **色彩收编**。现有 68 处字面色、40+ 不同色值全部归入语义 token（§3.1），UnwrapStage 是重点收编区。
- **排印纪律**。字号与字距、行高成组定义（§3.2）：大标题负字距，小字正字距，不再出现脱离层级的散值。

风险：低。两轮 hotfix 刚收敛的按钮/pill/术语体系全部保留，改动是"收编"而非"重画"。

### 2.2 备选方向 B：「深夜唱片店」（仅供定稿对比）

深底（暖黑）为主、奶油色降为点缀、金更亮；大字距对比更强的编辑排版；仪式感靠"暗场中一束光"。视觉上更独特，但：深色优先与"礼物 = 白天拆开"的心智相悖；需要重做全部对比度验证；与现行基线差异大，迁移成本高。**不推荐**，列出仅为定稿时确认"我们不是没考虑过别的"。

> **定稿决策点 ①**：方向 A（推荐）/ 方向 B。

---

## 3. Design Tokens

### 3.1 颜色

语义三层：`canvas`（页面框架外）→ `bg`（页面底）→ `surface`（卡片/浮层）。文字三级。强调色唯一。

| Token | Light | Dark | 用途 |
|---|---|---|---|
| `--canvas` | `#e8e2d8` | `#130f0c` | 页面框架外 |
| `--bg` | `#f7f3ec` | `#1a1410` | 页面底（现行值保留） |
| `--surface` | `#fffefb` | `#241c15` | 卡片、输入区 |
| `--surface-2` | `#f0ebe1` | `#2e241b` | 嵌套层、hover 底 |
| `--text` | `#2a1f14` | `#f2e9db` | 主文字 |
| `--text-2` | `#7a6a5a` | `#b8a793` | 次要文字 |
| `--text-3` | `#b0a090` | `#7d6e5c` | 占位/禁用/元信息 |
| `--accent` | `#c8956c` | `#d4a574` | 强调（现行值保留） |
| `--accent-action` | `#c4874f` | `#e0b07a` | 主按钮 |
| `--accent-tint-12/24/40` | accent @ 12%/24%/40% | 同 | 激活底、tag、进度 |
| `--danger` | `#c0554a` | `#c0554a`（待提亮） | 删除/错误（收编现有 5 处字面色；dark 提亮 #d4776c 留待 Phase 2 决策） |
| `--border` | `#e4dbd0` | `#3a2a1c` | 分隔线 |
| `--focus-ring` | accent @ 50% | 同 | `:focus-visible` |

> Phase 1 实施注记（2026-08-08）：上表 hex 已按 §9②「对照现行样式微调后锁死」与代码 token 层对齐（surface-2/text-2/text-3/dark canvas/dark accent-action/dark border 六处由文档初始值改为现行渲染值）。另增实施期补充的场景 token（材质 glass/dim/scrim、辉光 glow 两档、danger-tint 系、UnwrapStage/vinyl 场景色），明细见 styles.css `:root` 与 dark token 块——文档不逐条复制，以代码 token 区为准。

规则：

- 代码中**禁止出现字面 hex/rgba 色值**（本文档表格除外）。lint 阶段可用 stylelint 或简单 grep 守门。
- 深色主题不是浅色的反色，是独立设计的一档：金提亮、阴影加深、纸面纹理降噪。dark 覆盖段只许覆盖 token，不许写组件级覆盖（现有 `styles.css:1801-1819` 的组件级覆盖要收编）。
- 玻璃材质 = `surface` @ 60% + `backdrop-filter: blur(20px) saturate(180%)`。**浅色玻璃不得叠浅色玻璃**；大表面的模糊和阴影重于小表面。

### 3.2 排印

字号、行高、字距**成组定义**，禁止单独覆写字距。Cormorant Garamond 仅拉丁；中文标题落 `Songti SC / STSong` 衬线栈，正文 `-apple-system / PingFang SC`。

| 层级 | 字号/行高 | 字距 | 字重/字体 | 用途 |
|---|---|---|---|---|
| `display` | 34 / 1.1 | -0.02em | serif 500 | 礼物页 hero 标题 |
| `title` | 24 / 1.2 | -0.015em | serif 500 | 页面标题、稿件标题 |
| `headline` | 17 / 1.35 | -0.01em | sans 600 | section 头、卡片标题 |
| `body` | 15 / 1.6 | 0 | sans 400 | 正文、对话气泡 |
| `caption` | 13 / 1.45 | +0.01em | sans 400 | 辅助说明、元信息 |
| `caption-strong` | 13 / 1.45 | +0.01em | sans 600 | 卡片分区头、列表项标题（§10 工作台卡片头） |
| `label` | 11 / 1.3 | +0.06em | sans 600 大写 | 状态徽章、分组标签 |
| `lyric` | 17 / 2.0 | +0.02em | 宋体栈 | 歌词/LRC（现行手稿感保留） |

### 3.3 形状 / 阴影 / 间距

- 圆角五档：`--r-sm: 8`（控件）、`--r-md: 12`（chip/输入）、`--r-lg: 16`（卡片）、`--r-xl: 20`（sheet/hero）、`--r-pill: 100`。现有 10/14px 两档就近归并。
- 阴影两级 + 材质阴影：`--shadow`（卡片）、`--shadow-lg`（浮层/modal）、玻璃层在 `--shadow` 之上加深 20%。暖棕低透明度基调不变。
- 间距 4pt 网格：4/8/12/16/24/32/48，组件内边距与组件间距只许取这组值。

---

## 4. 动效系统（Motion）

### 4.1 为什么引入 Motion

apple-design 的核心要求——**可中断、从当前值起步、速度交接、动量投影**——CSS keyframes/transition 结构性做不到（无法中途接管、无法携带速度）。Motion（`motion` 包，~18KB gzip）的弹簧 API 与 Apple 的 damping/response 模型一一对应，是本方案的动效基座。CSS 仅保留：无限循环类动画（spin、typing dots、wave、呼吸提示）与 hover 变色。

### 4.2 弹簧预设（全站只有这四档）

| 预设 | Motion 参数 | 对应 Apple | 用途 |
|---|---|---|---|
| `spring-default` | `bounce: 0, duration: 0.4` | damping 1.0 / response 0.4 | 位移、出现/消失、布局变化——**默认档，无回弹** |
| `spring-snap` | `bounce: 0, duration: 0.25` | damping 1.0 / response 0.25 | 控件反馈：tab 切换、chip 选中、checkbox |
| `spring-sheet` | `bounce: 0.2, duration: 0.35` | damping 0.8 / response 0.3 | modal/sheet/抽屉——**只有带动量的手势才允许回弹** |
| `spring-pop` | `bounce: 0.3, duration: 0.4` | damping 0.7 / response 0.4 | 徽章出现、成功对勾、点赞——小而明确的愉悦 |

规则：

- **回弹只给有动量的交互**。淡入的菜单不许 overshoot；被快速滑动甩出去的卡片可以。
- 二维运动拆成独立 x/y 弹簧。
- 手势结束 → 弹簧起步速度 = 手指释放速度（Motion `velocity` 选项直接接收 px/s）。
- 决定"提交还是回弹"用释放速度的**符号**，不用位置。
- 边界橡皮筋：`rubberband(overshoot, dim, 0.55)`（用于播放器进度拖到头、LRC 拉到底）。
- 空间一致性：从哪来回哪去（面板右进右出）；可逆过渡镜像缓动。
- 手势 1:1 跟踪用 Pointer Events + `setPointerCapture`，尊重抓取点偏移。

### 4.3 三个仪式时刻的编舞

**① 拆礼物（UnwrapStage）**——产品最重要的一屏，从"播 keyframes"改为"弹簧编排"：

1. 待机：礼盒呼吸浮动（保留 CSS `boxIdle`），尘埃粒子漂浮（降级为 transform/opacity，粒子数 12→8，省主线程）。
2. pointer-down 瞬间：礼盒 `scale(0.98)`（100ms），给出"抓到了"的确认。
3. tap 提交：盖子以 `spring-sheet` 向上分离（初速度给 200px/s，轻微过冲），光爆 = blur(0→12px) + scale(0.9→1.15) + opacity 三段 materialize，丝绒面在光中淡出。
4. 整场动画任何时刻可被再次点击跳过：所有运动从当前值起步直接快进收尾——这是可中断原则在仪式时刻的兑现（仪式不能变成"强制看完"）。
5. 收尾：sessionStorage 写入与卸载时机保持现有 React 状态机（注释中记录的 remove() 白屏教训不回退）。

**② 拆开后级联入场（GiftPage reveal）**：hero → countdown/player → LRC → 操作区，每块 `y: +12 → 0` + opacity，`spring-default`，块间 stagger 60ms。reduced-motion 下整块交叉淡入，无位移。

**③ 生成等待（MusicCard / GiftPage generating）**：等待态的原则是"状态可见、进度诚实"：

- 黑胶旋转、波形、typing dots 保留（无限循环类，CSS 即可）。
- 进度条宽度改弹簧驱动（目标值仍用现有非线性 cap-90% 公式，弹簧消除跳变）。
- 状态文案切换（writing_lyrics → elevate → review → creating_gift）用 200ms 交叉淡入，不用滑动。
- 失败出现用 `spring-pop` + 抖动（现有 shake 保留），并给出重试路径——等待态也必须回答"现在怎样了、我能做什么"。

### 4.4 Reduced motion / transparency / contrast

三信号各自独立响应，写进组件基座而非逐处补丁：

- `prefers-reduced-motion`：所有弹簧位移/缩放 → 200ms opacity 交叉淡入；回弹归零；无限循环动画停（现有 8 处覆盖保留并扩展）。
- `prefers-reduced-transparency`：玻璃层退化为 `surface` 实色，去掉 blur。
- `prefers-contrast: more`：`border` 提深一档，glass 层加 1px 实色描边。

---

## 5. 组件规范（在现有收敛成果上补状态层）

沿用 hotfix 2026_08_02c 的三档按钮 / 单一 pill 语言 / 术语表，本方案只补**交互状态**与**材质归属**：

| 组件 | 材质 | 状态规格 |
|---|---|---|
| `.btn-primary`（金胶囊） | 实色 accent | pointer-down `scale(0.97)` 100ms；hover 明度 +4%；loading 内嵌 spinner 不换文案位置 |
| `.btn-secondary`（描边） | 纸面 | 同上缩放；hover 底色 `--surface-2` |
| `.btn-ghost` | 无 | hover 底色 `--accent-tint-12`；danger 变体用 `--danger` |
| pill（选项） | 纸面 | 中性描边 → 选中 = accent 描边 + `--accent-tint-12` 底（现行不变）；**必须原生 `<button>`**（修 D19 键盘缺口） |
| 卡片 | 纸面 `--r-lg` + `--shadow` | hover 仅礼物/歌单卡片上浮 2px（`spring-snap`），内容卡片静止 |
| Modal | 玻璃 + 遮罩 dim | 入场 materialize：blur + `scale(0.96→1)` + opacity，`spring-sheet`；**Esc 关闭 + 焦点圈禁 + 关闭后焦点还原**（修 D19） |
| Header | 玻璃 | 内容从其下滚过；与内容交界用 blur/渐变边缘，不用 1px 硬分隔线 |
| AudioPlayer | 纸面 | 进度条 scrub：pointer 1:1 跟踪 + 两端橡皮筋；多播放器同页**播放互斥**（修 D19） |
| LRCViewer | 纸面手稿 | 当前行高亮为 accent，行切换 `spring-snap`；点击行 seek（改事件代理，去掉 `document.querySelector("audio")`，修 D19） |
| 状态徽章 | `--accent-tint` 系 / danger 系 | `label` 排印层；出现用 `spring-pop` |

内联 `style={{}}` 的 12 处全部收编为 class（ChatUI 金 pill、GuidedFlow/Studio 错误边距、进度条宽度、LRC 行高等）。

---

## 6. 流程层交互规格（只写新增/变更）

- **GuidedFlow**：场景 pill 双击触发两条聊天流 → 提交后 pill 立即 disabled（P0，既是体验也是 LLM 成本 bug）；`startChat` 加 AbortSignal，`useMusicGen` 加 unmount cleanup。质检指示器（elevate/review）文案切换走 §4.3③ 的交叉淡入。
- **ReviewCard → Studio 交接**：`stageStudioDraft` 切换时稿件卡以 `spring-default` 从上方进入，字段继承 ReviewCard 的最后位置感（空间一致性：内容"搬过来"而不是"刷新出来"）。
- **Studio**：AI 改稿字段高亮 `studioFieldFlash` 1.6s 保留；undo 按钮加 keyboard shortcut 提示。折叠动画保留 grid-rows 方案。
- **分享**：`handleShare` 加 clipboard 降级（execCommand / 显示链接手动复制）；liked 状态持久化到 localStorage。
- **i18n**：GiftPage "Untitled"、"for {name}"、生成失败英文串、PlaylistPage "Untitled" 全部入 i18n 表（五语言）。
- **sessionStorage 恢复**：`moment_guided` 等快照读入时做形状校验，版本漂移则丢弃重来，不许渲染 crash。
- **CountdownFrame**：生成内容自带风格属"礼物内容"而非"产品界面"，**不在本系统管辖范围**；只规范其占位态（spinner → 弹簧进度）与失败态。

---

## 7. 修复优先级（与 D19 对齐）

| 优先级 | 项 | 出处 |
|---|---|---|
| P0 | pill 双击双流、startChat 无 AbortSignal、useMusicGen 无 cleanup | §6 |
| P0 | 键盘可达性：role="button" 全部改原生 button（ChatUI/MusicCard/ReviewCard/LRCViewer） | §5 |
| P0 | LoginModal Esc / 焦点管理 | §5 |
| P0 | sessionStorage 快照形状校验 | §6 |
| P1 | AudioPlayer 互斥、play() reject catch、share 降级、liked 持久化 | §5/§6 |
| P1 | i18n 硬编码残留 | §6 |
| P2 | LRC seek 事件代理化、review 报告持久化、polish provider 硬编码 | §6 |

---

## 8. 落地路线（定稿后按序执行）

1. **Phase 1 · 地基（无视觉变化）**：装 `motion`；建 tokens（§3 全表 + dark 对等值）；grep 收编 68 处字面色与散档圆角；内联样式收编。验收：`grep -c '#[0-9a-f]\{6\}' styles.css` 仅剩 token 定义区。✅ 2026-08-08 完成
2. **Phase 2 · 状态层 + P0 修复**：按钮/pill/卡片的按压与 hover 态；原生 button 替换；Modal Esc/焦点；P0 四项。视觉变化刻意最小。
3. **Phase 3 · 创作页工作台（§10，todo #2）**：宽双栏 IA + 选区改稿 + take 试听区 + 引导模式宽形态 + 移动端 dock/sheet。
4. **Phase 4 · gift 页音乐化（todo #1，含仪式动效）**：UnwrapStage 弹簧编排、reveal 级联、生成等待态、AIGC 背景链路。依赖生成链路改造，单独排期。
5. **Phase 5 · P1/P2 收尾**：播放器互斥、分享降级、i18n 收编等。

每期独立可合并、可回滚；Phase 1–2 不应引起任何截图级差异。

---

## 9. 定稿决策记录（2026-08-08）

- [x] ① 视觉方向：**A 暖纸·金光**（用户经 `/prototype/design` 三变体原型对比选定；方向 B 废弃）
- [x] ② 色板 hex 值：按 §3.1 表格生效，Phase 1 实施时对照现行样式微调后锁死
- [x] ③ 排印七层级：按 §3.2 生效（hero display 34px）
- [x] ④ 四档弹簧预设：按 §4.2 生效（`spring-pop` bounce 0.3）
- [x] ⑤ UnwrapStage 可随时点击跳过：**接受**（可中断原则优先于仪式完整性）
- [x] ⑥ P0/P1 优先级：按 §7 生效
- [x] ⑦ Motion 依赖（~18KB gzip）：确认引入
- [x] ⑧ 创作页工作台（§10，2026-08-08 追加）：路线 B 创作页整体宽屏化（左驱动右产物）；经 `/prototype/studio` 原型两轮评审定稿——v2 修正确认：单一卡片语言统一双栏、移动端编辑/试听AI 双态、引导模式快捷 pills 归位、undo 钮仅创作室

---

## 10. 创作页工作台（CreatePage Workbench）

> 状态：**已定稿 2026-08-08**。经 `/prototype/studio` 原型三视图（创作室桌面 / 引导桌面 / 移动双态）两轮评审确认：v1 指出"双栏不统一、移动看不出流程"，v2 以单一卡片语言 + 移动双态修正确认。对应 `docs/todo.md` #2；todo #1（gift 页音乐化）后置，见 §8 Phase 4。
> 设计输入（头脑风暴四问结论）：痛点 = 播放器/手稿/AI 三区单栏来回跳；设备 = 两端并重；AI 角色 = 分层（嵌入指令 + 聊天）；试听/版本 = 第一公民。

### 10.1 目标与空间语法

把"ChatGPT 写词 ↔ 优化 prompt ↔ Suno 生成 ↔ 手动修改"的多 App 往返收进一个页面闭环。一副骨架服务两个模式：

- **左栏 = 驱动区**：引导模式是对话流，创作室是手稿——切 tab 时框架不动，左栏内容弹簧交叉替换
- **右栏 = 产物区**：引导模式是需求卡→审核卡→生成卡（按阶段出现），创作室是试听卡 + 版本卡 + AI 协作卡

### 10.2 框架与布局

- **路由级宽度**：创作页（`/`）在视口 ≥1100px 时框架宽 1040px；其余路由保持 720px 不变。宽度只随路由变、同页内永不变（与 2026_08_05b「同页不跳宽」约束兼容：tab 切换两模式同宽，故无跳宽）；路由间宽度差以 `spring-default` 过渡。
- **双栏**：`grid-template-columns: 440px 1fr`；左栏右边线 1px `--border`；右栏底色 `surface-2 @ 30%` 与左栏区分"舞台"感。
- **单一卡片语言**：两栏所有内容块 = 同一种卡（`--r-lg`、padding 18–20px、`--shadow`、间距 16px 节奏）。禁止卡上叠卡（气泡无投影用 `--surface-2` 平色）；列表收进单卡内（版本卡分隔线行，禁止每行独立带边框小卡）。
- **概念标签不上 UI**：分组靠卡片语言与间距表达（"驱动区/产物区"是设计语言，不是界面文案）。
- **顶栏**：模式 tab（引导创作 | 创作室，pill 分段控件）+ 右侧草稿状态（"草稿已保存 · HH:MM"）；**undo 钮仅创作室出现**（撤销 AI 改稿，引导模式无此概念）。

### 10.3 创作室（Studio）

- **手稿卡**：标题印在纸上（无边框衬线输入，卡内顶部）→ 1px 分隔线 → 歌词正文（宋体手稿排印，§3.2 lyric 层）。整张卡 = "一张纸"。
- **风格卡**：已选 chips + 建议 pills + ↻ 重抽（沿用现有交互）。
- **操作行**（卡外）：`保存`（轻 PATCH）+ `保存并重新生成`（重 PATCH + 再生成），编辑模式双路径沿用现有语义。
- **试听卡**：播放器（碟片 + 标题/版本/时长）+ **进度条选段**（§10.4）。
- **版本卡**：每次生成 = 一条 take 行（徽章 V# + 说明 + 时间）；当前版本高亮 `--accent-tint-12`；行内操作：试听、**从这一版分叉**（= 现有"载入到草稿"的更名，快照进手稿）。与 `getGiftVersions` / `regenerateGift` 对齐。
- **AI 协作卡**：气泡流 + 内嵌输入条（border-top 分隔）；bot 气泡 `--surface-2` 平色。

### 10.4 选区改稿（核心新交互）

两个入口，同一个机制：

1. **文本划选**：手稿中划选歌词行 → 浮动工具条（深色 pill：改写 / 更押韵 / 更口语 / 缩短 / 自定义指令…，`spring-snap` 浮现于选区上方，尾尖指向选区）。
2. **音频选段**：试听卡进度条拖出时间范围（双 handle，选段带 accent 描边高亮）→ 经 LRC 时间轴**自动映射到歌词行**（"已选 0:42–1:05 → 对应歌词第 3–4 行"）→「交给 AI 修改」= 打开同一工具条流程。选段操作遵循 §4.2 手势规则（1:1 跟踪、橡皮筋边界）。

**提案 = 行内 diff**：AI 只重写选中行，手稿原位呈现 diff 块——旧行划线（danger 系淡底）、新行高亮（accent-tint-12）、底部操作条（"AI 提案 · 第 N–M 行" + ✓接受 / ✕拒绝）。接受 = 拼接手稿（先推 undo 快照，沿用 20 层栈）；拒绝 = 丢弃。多段提案逐段独立接受/拒绝。

**约束（必须坦诚写入 UI 语义）**：局部修改的是歌词，重新生成是**整首**——不存在音频级局部重生成。版本卡因此承担"对比与回退"职责。未来若接入支持 inpainting 的 provider，选区交互直接复用。

### 10.5 AI 协作分层

- **嵌入指令**（高频改稿）：选区工具条，零上下文成本，产出 diff 提案。
- **聊天**（开放讨论）：AI 协作卡，谈整体方向；**聊天产出的修改同样落为手稿行内 diff**，不再整字段静默替换。现有 `Done` 整字段替换协议保留兼容，新增 scoped 模式：请求带选区行号 + 上下文窗口，返回替换文本由前端按行拼接（`<<<LYRICS>>>` 标记惯例沿用，改为 `<<<LINES:N-M>>>` 携带范围）。
- ✨ 帮写保留（空手稿时的冷启动入口），落地为聊天内代发消息（现有机制不变）。

### 10.6 引导模式宽形态

- **左栏对话卡**（撑满栏高）：气泡流 + **快捷选项 pills 钉在输入条上方**（PillsRow 是引导模式的标志控件，与自由输入共存）+ 内嵌输入条。
- **右栏产物区按阶段出现**：需求卡（送给/场合/氛围/人声，随采集逐行填充）→ 歌词审核卡（质检徽章 + 歌词节选 + 「在创作室中打开」/「确认，生成音乐」）→ 生成进度卡。**ReviewCard/MusicCard 从聊天流中解放**，不再插在气泡之间；聊天流只管对话。
- 卡片出现/替换用 `spring-default` + 60ms stagger；reduced-motion 交叉淡入。

### 10.7 移动端规则

- **编辑态**（默认）：单栏手稿卡（真编辑界面）+ 风格卡 + 全宽主 CTA；**吸底迷你播放器**（碟片 + 标题 + 细进度条 + ▶ + ⌃）在有音频后常驻——滚动手稿音乐不停，glass 材质。
- **试听与 AI 态**：⌃ 上拉成 bottom sheet（`spring-sheet` 升起、可拖拽、速度交接、顶边橡皮筋、抓手 + 遮罩手稿），sheet 内容 = 桌面右栏全部（试听卡选段 / 版本卡 / AI 协作卡）；**sheet 内卡片与全局同一卡片语言，禁止任何 sheet 级覆盖**。
- **划选**：长按划选歌词行 → 紧凑工具条（改写/押韵/口语/缩短/指令…）；音频选段在 sheet 内同桌面。
- tab 分段控件吸顶；undo 入口进 AI 协作卡头部（移动无顶栏位置）。

### 10.8 动效编舞（全部走 §4.2 预设）

| 时刻 | 预设 | 说明 |
|---|---|---|
| tab 切换 | `spring-default` | 框架不动，左栏内容交叉替换 |
| 工具条浮现 | `spring-snap` | scale 0.96→1 + fade，尾尖指向选区 |
| diff 提案出现 | `spring-default` | 提案块展开（grid-rows）+ 操作条淡入；系统行为无回弹（§4.2） |
| 接受/拒绝 | `spring-snap` + `spring-pop` 脉冲 | diff 块收编进正文/消失；undo 钮以 pop 短暂脉冲（成功确认时刻，允许回弹） |
| 底部 sheet | `spring-sheet` | 手势驱动，可中断可反向，速度交接 |
| 卡片阶段出现 | `spring-default` + 60ms stagger | 引导右栏 |
| 选段 handle | 手势 1:1 | 边界橡皮筋（§4.2 公式） |

### 10.9 状态与后端对齐

- 草稿：`moment_studio_draft` / `moment_guided` sessionStorage 持久化不变（快照形状校验见 §7 P0）。
- 编辑模式：双保存路径（轻 PATCH / 重 PATCH + regenerate）不变；409 冲突提示不变。
- `streamChat`：新增 scoped 模式（`mode: "studio_scoped"`，请求带 `selection: {from, to}` 行号 + 上下文），返回 `<<<LINES:N-M>>>` 替换块；现有 `mode: "studio"` 与 Done 协议不动。后端 prompt 需保证只输出选中行的替换文本。
- take 分叉 = `loadVersionToDraft`（现有）更名与入口移位，无 API 变更。
- 新建流程生成后不再直接跳走：产物区出现试听卡（MusicCard 语义并入试听卡 + 版本卡）。
- 图标统一走 `Icons.tsx` 矢量集（原型稿中的文本字形 ▶➤⑂⌃ 等仅为占位，实现时不得照搬）。

### 10.10 验收清单

- [ ] 桌面 ≥1100px：创作页 1040 双栏，其余路由 720 不变；tab 切换无宽度动画外的任何跳变
- [ ] 两栏所有内容块为单一卡片语言（圆角/内距/阴影/节奏一致），无概念标签文案
- [ ] 手稿划选 → 工具条 → scoped 请求 → 行内 diff → 接受/拒绝/undo 全链路可用
- [ ] 试听卡进度条可拖选段并正确映射 LRC 歌词行（含纯音乐无 LRC 的降级：隐藏映射提示）
- [ ] 版本卡：生成产生新 take；分叉载入快照进手稿；回听切换当前版本
- [ ] 引导模式右栏三卡按阶段出现，ReviewCard/MusicCard 不再出现在聊天流内
- [ ] 移动端：dock 常驻不停听；sheet 上拉/拖下可中断；长按划选工具条可用
- [ ] 全部动效走 §4.2 四档预设，reduced-motion 降级交叉淡入
- [ ] 顶栏 undo 仅创作室；引导/创作室状态互不泄漏
