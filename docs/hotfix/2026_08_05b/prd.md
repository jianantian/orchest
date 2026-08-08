# Hotfix 2026-08-05b PRD:单栏对话式创作室

## 背景

2026_08_05 的创作室(双栏:左表单 + 右 AI 面板)用户实测"还是很难用":

1. **引导页太宽**:1080px 框架里 520px 聊天列两侧大片死白;
2. **创作室左右不对齐**:两栏顶线/基线不齐,视觉粗糙;
3. **AI 侧栏两难**:双栏/覆盖抽屉/浮窗三种形态都被否决——本质是 AI 与草稿"抢同一块屏幕";
4. **手动面板不完整**(对标 Suno):风格建议刷新、AI 帮写等基本辅助被砍,不开 AI 就是残血面板。

**设计定稿(已与产品确认):单栏对话式创作室。** 产品已有语言"对话是主线,草稿是对话里的活卡片"(引导模式 ReviewCard 已验证)——创作室与引导模式同构:

- 单栏自上而下:**稿件卡**(标题 + 可折叠歌词/风格/更多选项 section + 生成按钮)→ **AI 协作区**(可整体折叠)→ 钉底输入框;
- AI 协作区折叠 = 完整 Suno 式手动面板(歌词带 ✨帮写、风格带建议+刷新、更多选项收人声/器乐);
- AI 协作区展开 = 对话修改一切,自动应用到稿件卡并高亮,撤销保留;
- ✨ 快捷按钮 = 向对话**代发**一条消息(AI 只有一个出口,可追问);
- **全站框架统一 720px**(治引导页太宽;播放列表/我的 600 居中不变);
- 移动端不再需要"草稿/AI"双 tab,单栏自然适配。

品牌约束(preserve):奶油底 + `--gold` 唯一强调色 + 衬线标题层 + pill 100px/卡片 16px 圆角体系不动;动画只做 transform/opacity,尊重 `prefers-reduced-motion`;新文案零 em-dash。

## 目标

1. 全站桌面框架 720px,guided 聊天列放宽,切页无跳变;
2. 创作室单栏:稿件卡 + AI 协作区 + 钉底输入框;AI 区可折叠,折叠后手动面板功能完整;
3. Suno 式可折叠 section:歌词(✨帮写)、风格(建议 6 + 刷新)、更多选项(人声/器乐,默认折叠);
4. 编辑模式(?edit=id)在新结构中原位保留;AI 自动应用/撤销/高亮机制不变。

## 成功指标

- 2 个 issue 关闭,验收 checklist 全勾
- `npm run build` 通过
- 截图验证:720 框架下 guided/studio/playlist/mine;AI 区折叠/展开;section 折叠;✨ 代发;移动端单栏

## Issue 拆分

| Issue | 标题 | GitHub | 依赖 |
|-------|------|--------|------|
| 001 | 720 框架 + 单栏创作室结构 | [#283](https://github.com/jianantian/orchest/issues/283) | 无 |
| 002 | Suno 式 section 与 AI 协作折叠交互 | [#284](https://github.com/jianantian/orchest/issues/284) | 001 |

按 WORKFLOW:分支 `hotfix/2026_08_05b-single-col-studio`(worktree `.worktrees/hotfix-single-col-studio`),一 issue 一 commit,`closes #N`。

## 范围裁定

- 只动 `examples/demo/music-gift/frontend/`
- AI 协作的后端协议、自动应用/撤销逻辑不动;引导模式交互不动(仅随框架放宽)
- 稿件卡不做 sticky(长歌词时 sticky 会吃掉对话视口;歌词 section 折叠即解决了卡片高度问题)——改动可见性由气泡内的 applied-note + 高亮闪烁承担
- 风格建议恢复为 section 内常驻一行(6 个 + 刷新),取代 2026_08_05 的空态轻建议(section 可折叠已容纳噪音)
- 对话历史不持久化(维持原裁定);section/AI 区的折叠状态也不持久化
