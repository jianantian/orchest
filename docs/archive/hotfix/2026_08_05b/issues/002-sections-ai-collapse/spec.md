# 002 — Suno 式 section 与 AI 协作折叠交互

## 背景

001 把创作室改成单栏。本 issue 把手动面板补全为 Suno 级完整(用户核心诉求:"折叠时功能也应该是完整的,和 suno 类似"),并给 AI 协作区加折叠——折叠后 = 完整手动面板,展开后 = 对话修改一切。

## 目标/范围

`examples/demo/music-gift/frontend/src/`:`Studio.tsx` + `styles.css` + `i18n.tsx`(五语言)。

1. **稿件卡 section 化**(Suno 式可折叠 section,chevron 旋转动画,transform/opacity,`prefers-reduced-motion` 退化为无动画):
   - **歌词 section**(默认展开):header = chevron + "歌词" + 右侧工具(✨ 帮写);body = 现有 manuscript textarea(器乐禁用逻辑不变);歌词为空时 body 底部显示"帮我写歌词" pill;
   - **风格 section**(默认展开):header = chevron + "风格";body = 已选 chips + 行内"+ 风格"输入 + **建议一行(6 个 + ↻ 刷新,常驻 section 内)**——取代 2026_08_05 的空态轻建议(`shuffleStyles` 池,刷新重取);
   - **更多选项 section**(默认折叠):header = chevron + "更多选项";body = 两行选项:"演唱方式"[演唱|器乐]、"人声"[女声|男声](器乐时禁用)——取代 #281 的三态属性条,内部 state(instrumental/vocalGender)不变;
   - 标题 input 与生成/保存按钮保持在 section 之外(卡片顶部/底部)。
2. **AI 协作区折叠**:header = chevron + "AI 协作" + 右侧 ↩ 撤销按钮(从工具条迁入);折叠时气泡流与输入框整区隐藏;默认展开。
3. **✨ 代发**:点"帮写/帮我写歌词"→ 若 AI 区折叠则展开,然后以当前界面语言向对话代发一条自然语言请求(中文"帮我写一首歌词"等,五语言各一条预设文案),走现有 `sendTurn`;AI 区滚动到底。
4. i18n:`more_options`(重新加入)、`write_for_me`("帮我写歌词")、`ai_help_write`(✨ 工具 title)、代发预设文案 `ai_prompt_write_lyrics`、`vocal_mode`(演唱方式)、`vocal_sung`(演唱)等,五语言;清理被取代的死 key(空态建议相关,若不再用)。
5. 撤销逻辑、applyDone、高亮闪烁目标(歌词 textarea/风格 section/标题/更多选项内 pills)适配新结构。

## 验收标准

- [ ] 三个 section 可折叠/展开,chevron 动画正常;更多选项默认折叠;reduced-motion 下无动画
- [ ] 风格 section 内 6 个建议 + 刷新常驻;点击成 chip;清空风格后建议仍在(section 内)
- [ ] 歌词为空时显示"帮我写歌词" pill;点击后 AI 区展开(若折叠)并代发请求,AI 开始流式回复
- [ ] AI 协作区可整体折叠/展开;折叠时页面 = 纯手动面板(标题/歌词/风格/更多选项/生成按钮),功能完整
- [ ] 撤销按钮在 AI 区 header,功能不变;AI 应用改动后对应 section 内控件高亮
- [ ] 演唱/器乐与人声两行选项行为与 #281 三态一致(器乐禁歌词、人声在器乐下禁用)
- [ ] 移动端正常;浅色/深色无破版;`npm run build` 通过

## Notes

- 折叠状态不持久化(PRD 裁定)。
- 代发文案是自然语言用户消息,studio 系统提示词已能处理;不需要新后端。

## 实施步骤(plan)

读:001 产出的单栏 `Studio.tsx`、`styles.css` 单栏 studio 段、`i18n.tsx`。

改:

1. `Studio.tsx`:section 容器组件(或本文件内小函数)+ 三个 section;AI 区折叠 state;✨ 代发(调 `sendTurn` 前展开 AI 区);撤销迁入 AI header;建议行恢复(state: 6 个 + refresh);更多选项两行选项。
2. `styles.css`:`.draft-section` 系列(header/chevron/body/折叠动画)、AI 区折叠、建议行。
3. `i18n.tsx` 五语言增删。
4. `npm run build`;agent-browser 截图:section 折叠、✨ 代发(可观察 AI 区出现气泡)、AI 区折叠、移动端。
