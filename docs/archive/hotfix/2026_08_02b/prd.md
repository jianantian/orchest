# Hotfix 2026-08-02b PRD:music-gift 创作室(二次编辑 + 自由创作重构)

## 背景

music-gift demo 目前两个创作入口都存在能力缺口:

1. **生成的作品无法二次编辑**。歌词错一个字、风格不满意,只能从头走一遍引导流程重新生成。
2. **自由创作模式的 AI 能力是割裂的一次性工具**——"写歌词/改歌词/扩写/polish 风格"四个按钮各自独立单次调用,无对话记忆,系统提示词硬编码在前端(`FreeCreatePanel.tsx`),用户无法通过对话迭代作品。

设计决策(已与产品确认):

- **创作室 (Studio)**:自由创作重构为统一工作区,草稿(歌词/风格/人声/标题)是唯一状态源,**手动编辑**与 **AI 协作**同为第一优先级操作。
- **AI 协作是对话式多轮**:可以让 AI 按要求改歌词、改风格 prompt;AI 修改**自动应用进草稿**(应用前快照,可撤销),不再走"提案-确认"。
- **布局**:桌面端双栏(草稿编辑区 + AI 对话面板同屏),移动端折叠为 tab 切换。
- **质量管道**:Studio 内跳过引导模式的 elevate/review 质量管道(否则用户的定向修改会被润色管道重写,且每轮多等几十秒);引导模式保持现状。
- **版本化**:编辑后重新生成在**同一 gift 内产生新版本**,旧版本保留可查可听,作品列表不膨胀,默认展示最新版。分享链接不变,链接内容随最新版更新。
- **字段**:歌词/风格/人声修改需重新生成;标题是轻编辑(只更新 meta,不重新生成);封面随再生成由 provider 附带更新。

## 目标

1. 创建者可进入创作室编辑已有作品,保存并重新生成,同一 gift id 原位产生新版本,旧版本可回听。
2. 自由创作重构为创作室:手动编辑与对话式 AI 协作平权,AI 可改歌词/风格/人声/标题并自动应用、可撤销。
3. 编辑入口:GiftPage 作品页(owner)+ MyGifts 列表页双入口;引导模式 ReviewCard 可"在创作室中打开"。

## 成功指标

- 4 个 issue 全部关闭,各 spec 验收 checklist 全勾
- `cargo test -p music-gift-demo`(或对应包名)通过,后端新端点有单元测试
- `frontend/` 下 `npm run build`(tsc + vite)通过
- 浏览器冒烟:AI 对话改歌词自动应用+撤销;编辑已有作品→重新生成→版本切换回听 V1

## Issue 拆分

| Issue | 标题 | GitHub | 依赖 |
|-------|------|--------|------|
| 001 | 后端:gift 版本化 + 编辑/再生成/版本查询端点 | [#272](https://github.com/jianantian/orchest/issues/272) | 无 |
| 002 | 后端:/api/chat studio 协作模式 | [#273](https://github.com/jianantian/orchest/issues/273) | 无 |
| 003 | 前端:创作室重构(统一草稿 + AI 协作面板) | [#274](https://github.com/jianantian/orchest/issues/274) | 002 |
| 004 | 前端:二次编辑闭环(编辑入口 + 版本切换) | [#275](https://github.com/jianantian/orchest/issues/275) | 001, 003 |

001/002 后端相互独立;003 依赖 002 的 studio 模式协议;004 依赖 001 的端点与 003 的创作室。

按 WORKFLOW:实施分支 `hotfix/2026_08_02b-gift-studio`(worktree `.worktrees/hotfix-gift-studio`),一 issue 一 commit,commit message 带 `closes #N`。

## 范围裁定

- 只动 `examples/demo/music-gift/`,不动 `crates/` 核心与协议契约
- 引导模式(GuidedFlow)保持现有行为,仅 ReviewCard 加一个跳转入口(004)
- 版本化只在 gift 内部,不 fork 新 gift;不做版本间的 diff 视图
- 生日倒计时页内嵌的歌词片段是创建时快照,**不随**再生成更新(已知限制,不做)
- 再生成期间旧音频暂时不可用(作品短暂离开播放列表),接受此行为,不做"保留旧音频待新版就绪再切换"
- Studio 的 AI 对话历史不持久化(刷新清空);新建模式的草稿用 sessionStorage 持久化
- 不做封面的单独重生成入口(封面随音乐再生成附带更新)
