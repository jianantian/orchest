# Hotfix 2026-08-02c PRD:music-gift 前端一致性收敛

## 背景

创作室(2026_08_02b)上线后,产品出现明显的一致性问题(用户反馈"整个产品没有一致性和统一感了"):

1. **布局跳动**:引导模式内容列 520px,创作室通过 `#root:has(.studio)` 撑到 1080px,切 tab 时整个框架含顶栏宽度突变;
2. **滚动模型不统一**:部分页面 `.app-main` 单滚动,guided 聊天内部滚动,创作室两栏各自独立滚动(同屏两个滚动条);
3. **控件方言多**:按钮至少 5 种样式(金胶囊/chunky 分段控件/幽灵小字/描边/圆图标),chip/选项样式 4-5 种各自为政;
4. **术语漂移**:tab 叫 "Free"、面板叫 "AI Partner"、文档叫"创作室"。

方向(已与产品确认):不做重新设计,以现有奶油色 + 衬线 editorial 风格为基准收敛;**统一宽版布局**(桌面 ~1080px,引导聊天列居中 520px)+ **完整收敛**(布局/控件/术语/滚动四项)。

## 目标

1. 全站框架宽度统一,切 tab/切页顶栏与框架不跳;每屏只有一个滚动条,聊天输入框始终钉底/吸附底;
2. 按钮收敛为 3 档(主金胶囊/次描边/幽灵文字),选项类控件收敛为 1 种 pill 语言,图标按钮 1 种圆形幽灵;
3. 术语统一:tab "Free" → "Studio/创作室",AI 面板标题改为 "AI 协作/AI Co-writer"。

## 成功指标

- 2 个 issue 关闭,验收 checklist 全勾
- `npm run build`(tsc + vite)通过
- 桌面截图审计:guided/studio/playlist/mine 四页框架同宽、视觉语言一致;浅色/深色各抽验一页

## Issue 拆分

| Issue | 标题 | GitHub | 依赖 |
|-------|------|--------|------|
| 001 | 布局宽度与滚动模型统一 | [#277](https://github.com/jianantian/orchest/issues/277) | 无 |
| 002 | 按钮/选项控件与术语收敛 | [#278](https://github.com/jianantian/orchest/issues/278) | 001(同文件 styles.css,顺序提交) |

按 WORKFLOW:分支 `hotfix/2026_08_02c-ui-consistency`(worktree `.worktrees/hotfix-ui-consistency`),一 issue 一 commit,`closes #N`。

## 范围裁定

- 只动 `examples/demo/music-gift/frontend/`
- 礼物页(unwrap 沉浸体验)不动——刻意的仪式时刻,与品牌一致
- 移动端(<900px)行为不变(窄框架 + studio tab 切换)
- 不做配色/字体令牌调整,不做暗色专属调整(令牌层已支持)
- 不重构组件结构,只做样式与文案收敛 + 必要的 class 替换
