# 003 — 前端:创作室重构(统一草稿 + AI 协作面板)

## 背景

`FreeCreatePanel.tsx` 的 AI 能力是四个一次性工具(写歌词/改歌词/扩写/polish 风格):各自单次调用、无对话记忆、系统提示词硬编码在前端。按产品决策重构为**创作室 (Studio)**:草稿是唯一状态源,**手动编辑与 AI 协作同为第一优先级**;AI 协作是 002 提供的多轮对话模式,修改**自动应用**(应用前快照,可撤销);桌面双栏、移动 tab。

## 目标/范围

在 `examples/demo/music-gift/frontend/src/` 内:

1. **Studio 组件**(新组件,CreatePage 的 free tab 改渲染它;`FreeCreatePanel` 删除):
   - 草稿状态 `{ lyrics, selectedStyles, styleInput, vocal, title }` 集中在组件顶层,为唯一状态源;
   - 手动编辑区沿用现有控件与样式:歌词 textarea、风格 chips + 输入 + 建议、人声/器乐切换、标题输入、更多选项(人声性别)。
2. **AI 协作面板**:
   - 多轮对话(UI 复用 guided 的 bubble/typing-indicator 样式);每轮发送 `streamChat({ mode: "studio", draft: 当前草稿, messages: 全部历史, meta: { lang } })`;
   - `Delta` 流入当前 assistant 气泡(复用打字指示器;标记块之后的内容不在气泡显示,沿用 guided 的 `<<<LYRICS>>>` 截断逻辑);
   - `Done` 回传的非空字段(lyrics/style/title/vocal)**自动应用**进草稿:应用前把当前草稿整体快照压入撤销栈(上限 20),应用后对应字段高亮闪烁动画,气泡里附一句"已更新:歌词、风格"类确认;
   - 工具栏"撤销"按钮:弹出最近快照恢复草稿,栈空禁用。
3. **布局**:桌面(≥900px)双栏——左草稿编辑区、右 AI 面板;移动端顶部"草稿 / AI 协作"tab 切换。纯 CSS 媒体查询 + 条件类名,不引入新依赖。
4. **持久化**:新建模式草稿 sessionStorage 持久化(刷新不丢);AI 对话历史不持久化(PRD 范围裁定)。
5. **移除**:`lyrics_write/lyrics_edit/lyrics_expand/personalize` 四个一次性工具按钮、`handleAiAction/handleExpand/handlePolish` 及前端硬编码系统提示词;生成按钮逻辑不变(`useMusicGen.start`,instrumental/vocal 校验不变)。
6. i18n:新增 key(studio_ai_tab、studio_draft_tab、undo、applied_fields、ai_placeholder 等),en/zh 双语;删除不再用的 key。

## 验收标准

- [ ] free tab 渲染创作室:桌面双栏同屏可见,移动 tab 切换;手动编辑区控件功能与现状一致
- [ ] AI 对话多轮:发送"把副歌改短"后 `Done.lyrics` 自动写入歌词 textarea 并高亮;发送"换 City Pop 风格"后风格字段更新;未改字段保持不动
- [ ] 每次 AI 应用前可撤销:点撤销恢复应用前草稿;栈空时按钮禁用
- [ ] 纯对话(无标记)轮:气泡正常显示,草稿不变、不产生快照
- [ ] 新建模式刷新页面后草稿(歌词/风格/人声/标题)恢复;对话历史清空
- [ ] 生成流程端到端不变:studio 草稿 → 生成 → MusicCard → 跳转作品页
- [ ] 旧一次性工具按钮与 polish 调用全部移除,无残留引用;`npm run build`(tsc + vite)通过

## Notes

- 本 issue 只做"新建"形态;"编辑已有作品"(edit 模式、版本切换)在 004 接入,Studio 组件的 props 设计(可选 `editGiftId` 或由 004 包一层)在 004 定,003 不做预留抽象。
- 打字机逐字 reveal 可不做,直接 Delta 追加渲染(guided 的 rAF reveal 是为长歌词流优化的,studio 气泡是短文);若实测卡顿再补。

## 实施步骤(plan)

读:

- `frontend/src/components/FreeCreatePanel.tsx`(现有控件与生成逻辑)
- `frontend/src/components/GuidedFlow.tsx`(气泡、typing-indicator、标记截断、streamChat 用法)
- `frontend/src/hooks/useMusicGen.ts`、`frontend/src/api.ts`、`frontend/src/types.ts`
- `frontend/src/i18n.tsx`、`frontend/src/styles.css`

改:

1. `types.ts`:`ChatRequest` 类型加 `mode?: string; draft?: {...}`。
2. 新 `components/Studio.tsx` + `components/StudioChat.tsx`(或单文件两区,随体量定);草稿 state + 撤销栈 + 自动应用逻辑。
3. `pages/CreatePage.tsx`:free tab 渲染 Studio;删除 `FreeCreatePanel.tsx`。
4. `i18n.tsx`:新 key en/zh;清理死 key。
5. `styles.css`:双栏布局、移动 tab、字段高亮动画(`@keyframes`,尊重现有 `--*` 令牌与 `data-theme`)。
6. `lib/` 如需新增 `useSessionState` 类小 hook 放 `lib/` 或组件内,从简。
7. `npm run build`;agent-browser 冒烟:对话改歌词 → 自动应用 → 撤销;刷新恢复草稿。
