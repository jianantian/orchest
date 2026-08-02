# Hotfix 2026-08-02 PRD:music-gift 前端体验修复

## 背景

music-gift demo 日常使用反馈 4 个体验问题:

1. 主题只能跟随系统,没有浅色/深色手动选择入口
2. 引导(交互)模式最后一轮对话,发送后到"歌词生成中"类标签出现之间有大段无进度反馈的死寂时间
3. 生成完成卡片点击打开礼物后,返回对话页退回"生成这首歌"按钮,完成状态丢失
4. 浅色模式滚动条滑块与底色几乎同色,常驻 gutter 像一条错位白边

全部位于 `examples/demo/music-gift/frontend/`,不涉及 Rust 核心与协议契约。

## 目标

逐项修复上述 4 个问题,每项一个 GitHub issue、一个 commit,验收标准见各 spec。

## 成功指标

- 4 个 issue 全部关闭,各 spec 验收 checklist 全勾
- `frontend/` 下 `npm run build`(tsc + vite)通过
- 主题切换、生成状态恢复经浏览器冒烟验证

## Issue 拆分

| Issue | 标题 | GitHub | 依赖 |
|-------|------|--------|------|
| 001 | 浅色/深色模式手动切换 | [#267](https://github.com/jianantian/orchest/issues/267) | 无 |
| 002 | 引导模式流式进度指示补全 | [#268](https://github.com/jianantian/orchest/issues/268) | 无 |
| 003 | 生成完成状态跨页面恢复 | [#269](https://github.com/jianantian/orchest/issues/269) | 无 |
| 004 | 浅色模式滚动条配色 | [#270](https://github.com/jianantian/orchest/issues/270) | 无 |

4 项相互独立,按编号顺序实施。002/003 都改 `GuidedFlow.tsx`、001/004 都改 `styles.css`,在同一分支顺序提交避免冲突。

按 WORKFLOW:实施分支 `hotfix/2026_08_02-music-gift-ux`(worktree `.worktrees/hotfix-music-gift-ux`),一 issue 一 commit,commit message 带 `closes #N`。

## 范围裁定

- 只做 music-gift demo 前端,不动 `crates/`、不动 demo 的 Rust 后端
- 主题只加三态切换,不调任何配色令牌值
- 003 只恢复"生成完成/进行中/失败"卡片状态,不给 GiftPage 加返回按钮、不改自由创作面板
- 004 只改滚动条配色,不动 gutter 宽度与 header padding 补偿
