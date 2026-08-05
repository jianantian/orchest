# 001 — 手机 header 防折行与紧凑化

## 背景

手机(<900px)上 `.app-header` 的导航标签 `创作/发现/我的` 各自竖向折成两行,与主题切换按钮、语言下拉、头像/登录挤在一起,header 呈现两排混乱状态(用户截图实证)。乱因是折行与间距,不是内容多。

## 目标/范围

只动 `examples/demo/music-gift/frontend/src/styles.css`(header 相关段),如确有必要才微调 `App.tsx` 的 header 结构。

1. 导航链接(`.app-nav a` 或现有 class)加 `white-space: nowrap`;
2. 移动端(<900px)header 紧凑化:收紧水平 padding、缩小字号/图标间距,确保 logo + 3 个导航 + 主题 + 语言 + 头像单行放下(390px 宽实测);
3. 若 nowrap + 紧凑仍溢出:把主题切换与语言下拉在移动端收进用户菜单(头像下拉),header 只留 logo + 导航 + 头像。**优先不做这步**,实测决定;
4. 桌面端 header 样式不变。

## 验收标准

- [ ] 390px 宽:header 单行,`创作/发现/我的` 不折行,各控件不重叠不溢出
- [ ] 320px 宽:不破版(允许收进菜单,不允许折行/重叠)
- [ ] 桌面 header 视觉与现状一致
- [ ] 浅色/深色均正常
- [ ] `npm run build` 通过

## 实施步骤(plan)

读:`styles.css` 的 `.app-header` 段(~107-260)、`App.tsx` 的 header JSX;现有移动端媒体查询块。

改:

1. `styles.css`:导航 nowrap;`@media (max-width: 899px)` 内 header padding/gap/字号收紧。
2. 390px 与 320px 实测(agent-browser `set viewport`),溢出才做菜单收编。
3. `npm run build`;截图对比。
