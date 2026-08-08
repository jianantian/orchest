# 001 — 720 框架 + 单栏创作室结构

## 背景

1080px 框架下引导页两侧死白、创作室双栏不对齐。按 PRD 设计定稿:全站框架统一 720px,创作室改单栏(稿件卡 → AI 协作区 → 钉底输入框),消灭双栏与移动端双 tab。

## 目标/范围

`examples/demo/music-gift/frontend/src/`:`styles.css` + `Studio.tsx` + `CreatePage.tsx`(如需)。

1. **框架 720px**:`#root` 桌面媒体查询 1080→720;
   - guided `.create-page > .chat-panel` 从居中 520 放宽(去掉 520 上限,自然充满 720 框架内边距,约 640-680);
   - playlist/mine/auth 页保持 600 居中(720 内自然居中,无需改值);
   - gift 页不动。
2. **Studio 单栏重构**(保留 #281 的全部控件与逻辑,只重排结构):
   - 自上而下一栏:`.edit-head`(编辑模式,版本切换+播放器)→ 稿件卡(无边框标题 → 歌词 textarea → 属性条:风格 chips + 行内输入 + 空态建议 + 人声三态)→ 生成/保存按钮 → AI 协作区(工具条 + 气泡流)→ 钉底输入框;
   - AI 协作区**本 issue 先常驻展开**(折叠交互在 002);输入框保持 `position: sticky; bottom: 0`;
   - 删除双栏体系:`.studio-cols`、`.studio-draft-col`、`.studio-ai-col`(380px 基宽)、`.studio-mobile-tabs`、`.m-active`,以及 `Studio.tsx` 里的 `mobileTab` state;`CreatePage` 的 guided/Studio tab 保留;
   - 移动端:单栏自然流,无 tab;AI 输入框同样 sticky。
3. AI 协作的 streamChat/applyDone/undo/高亮逻辑、编辑模式的版本/保存/再生成逻辑零改动,仅随结构迁移。

## 验收标准

- [ ] 桌面:guided/studio/playlist/mine 同在 720 框架,切页无宽度跳变;guided 聊天列不再局促
- [ ] 创作室单栏:稿件卡在上、AI 对话在下、输入框 sticky 钉底;无双栏残留样式
- [ ] AI 对话改歌词/风格/标题/人声仍自动应用 + 高亮 + 可撤销(逻辑未动)
- [ ] 编辑模式:载入、版本切换、保存/再生成正常,布局不破
- [ ] 移动端 390px:单栏正常,无"草稿/AI"双 tab;浅色/深色无破版
- [ ] `npm run build` 通过

## 实施步骤(plan)

读:`styles.css`(#root 媒体查询、`.create-page > .chat-panel`、studio 段)、`Studio.tsx`、`CreatePage.tsx`。

改:

1. `styles.css`:720 框架;guided 放宽;删双栏/移动 tab 样式;单栏 studio 布局(稿件卡 + AI 区 + sticky 输入框)。
2. `Studio.tsx`:JSX 重排为单栏;删 `mobileTab`;其余逻辑原样。
3. `npm run build`;agent-browser 截图四页 + 移动端。
