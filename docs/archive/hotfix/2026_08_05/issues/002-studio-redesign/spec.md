# 002 — 创作室重设计(稿件 + 属性条 + 空态建议)

## 背景

创作室(2026_08_02b)把"所有能力同时平铺":左栏 7 个控件区,右栏 AI 又能做同样的事,双重入口互相抢视线。按 PRD 设计定稿重排:**稿件为主,属性收成一行,灵感收进对话**。布局宽度与滚动模型沿用 2026_08_02c(桌面 1080 框架、`.app-main` 单滚动、AI 输入框 sticky),不重议。

## 目标/范围

`examples/demo/music-gift/frontend/src/`,主要是 `Studio.tsx` + `styles.css` + `i18n.tsx`。AI 协作后端协议、自动应用/撤销逻辑不变。

### 左栏(稿件文档,四个视觉块)

1. **标题行**:无边框 input 直接以衬线标题样式呈现(placeholder 淡色"未命名"/现有 `free_title_ph`),focus 时下划线;不再是带 label 的表单 section;
2. **歌词编辑器**:textarea 保持 manuscript 样式,占左栏主体;器乐模式下 disabled(现状逻辑不变);
3. **属性条(一行,可换行)**:
   - 已选风格 chips(沿用 `.style-chip` 金填充);
   - 行内小型风格输入(placeholder "+ 风格",回车成 chip,行为同现有 `commitStyleInput`);
   - 人声三态 pill 组:`[女声|男声|器乐]`(沿用 `.opt-pill`,选中 `.on`);选"器乐"= 现有 instrumental 逻辑(歌词禁用、提交 kind=instrumental);女/男声 = vocalGender;**未选**= 与现状 `vocal` 未定义一致;
   - **空态轻建议**:风格为空(无 chips 且输入为空)时,属性条下方显示 4 个建议 pill(取自 `shuffleStyles` 池,一次取定,不带刷新按钮);一旦有值即消失。想要更多灵感 → AI 对话(panel intro 提示语可提一句);
4. **生成区**:新建模式 = 金胶囊主按钮(现有 `.btn-primary`);编辑模式 = 现有的保存/保存并重新生成按钮,随新布局原位排放。

### 右栏(AI 协作,固定窄栏)

- 桌面固定 ~380px(flex-basis),左栏占余量;AI 面板标题、气泡、sticky 输入框、撤销按钮保持现状;
- AI 改动继续自动应用到左栏对应控件(歌词/标题 input、风格 chips、人声三态),高亮闪烁保留;**风格 AI 值继续写入行内输入框(而非 chip)**,与 2026_08_02b 行为一致。

### 拆除

- Vocal/Instrumental 大分段控件(2026_08_02c 已改 pill,本次连 pill 位一并收进属性条);
- "更多选项"手风琴(性别并入三态);
- 常驻 14 个建议 chips + 刷新按钮;
- 标题独立 section(`.section-title` + input 两行结构);
- 产生的死 CSS、死 i18n key(`more_options`、`vocal_gender` 若不再用等,grep 确认后清理)。

### 编辑模式与移动端

- 编辑模式(`?edit={id}`):版本切换器 + 音频播放器保持在左栏顶部(现状位置),草稿区用新布局,保存/再生成按钮在生成区;
- 移动端(<900px):维持"草稿 / AI 协作"双 tab;草稿 tab 内结构 = 标题 → 歌词 → 属性条 → 生成按钮,同构。

## 验收标准

- [ ] 桌面创作室:左栏仅 标题/歌词/属性条/生成 四块;无分段控件、无手风琴、无常驻建议云
- [ ] 风格为空时显示 4 个轻建议;输入或选 chip 后立即消失;清空风格后重新出现
- [ ] 人声三态:器乐禁用歌词、女/男声与未选行为与现状一致(提交参数不变)
- [ ] AI 对话改歌词/标题/风格/人声仍自动应用到新控件并高亮;撤销可用
- [ ] 编辑模式:载入 gift、版本切换、保存(标题)/保存并重新生成 全部可用,布局不破
- [ ] 移动端双 tab 正常,草稿 tab 结构同构
- [ ] 浅色/深色无破版;`npm run build` 通过

## Notes

- 空态建议每次进入取一次 4 个即可,不做刷新、不做持久记忆。
- `suggestions`/`refreshSuggestions` 相关代码随建议云删除;`shuffleStyles` 保留给空态用。
- guided 模式不受影响。

## 实施步骤(plan)

读:

- `frontend/src/components/Studio.tsx`(现状全部)
- `frontend/src/styles.css`:`.free-panel`/`.editorial-section`/`.style-*`/`.more-options`/`.opt-pill`/`.title-input`/`.lyrics-manuscript`/studio 布局段
- `frontend/src/i18n.tsx` 相关 key

改:

1. `Studio.tsx`:按"左栏四块 + 右栏窄栏"重排 JSX;三态 pill 取代 instrumental/vocalGender 双 state(内部可保留两个 state,UI 统一为三态);空态建议逻辑;删除 more-options/suggestion cloud/title section。
2. `styles.css`:标题行样式(无边框衬线 input)、属性条布局、行内风格输入、AI 列 380px 基宽;删死样式。
3. `i18n.tsx`:"+ 风格" placeholder、空态提示等新 key(五语言);清理死 key。
4. `npm run build`;agent-browser 截图:桌面创作室(空态建议可见)→ 输入风格(建议消失)→ 编辑模式(用 `/?edit=<已有id>` 或注入)→ 移动端 390px。
