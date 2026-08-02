# Issue 001:浅色/深色模式手动切换

## 背景

music-gift 前端的主题完全由 `@media (prefers-color-scheme: dark)` 决定,用户没有任何手动选择入口——系统开深色就只能是深色,反之亦然。dark 块里除 CSS 变量外还有约 100 行硬编码组件覆盖,全部绑死在媒体查询上。

## 目标/范围

- 新增 自动/浅色/深色 三态主题选择,持久化在 `localStorage["moment_theme"]`
- 主题通过 `<html data-theme>` 属性驱动,`styles.css` 的 dark 块整体从媒体查询改写为 `html[data-theme="dark"]` 选择器,手动选择永远赢过系统偏好
- `index.html` 内联 boot script 在首屏前设置 `data-theme`,避免闪烁
- 入口放在顶栏语言切换旁,循环切换按钮(太阳/月亮/半圆图标),5 种语言 tooltip
- 设置 `color-scheme` 让 UA 控件(表单、滚动条)跟随主题

非目标:不改任何配色令牌值;不为每个页面单独做主题适配。

## 验收标准

- [ ] 顶栏出现主题切换按钮,点击按 自动 → 浅色 → 深色 循环
- [ ] 选择浅色/深色后,无论系统偏好如何,界面立即切换且刷新后保持
- [ ] 选择"自动"时跟随系统,系统主题变化时界面实时跟随
- [ ] 首次加载无主题闪烁(boot script 在 paint 前生效)
- [ ] 深色模式下所有原有视觉(unwrap 舞台、manuscript、按钮渐变等)与改前一致

## 实施要点

- `src/lib/theme.ts`:`ThemeChoice` 三态、`resolveTheme()`(auto 走 matchMedia)、`useTheme()` hook(auto 时监听系统变化)
- `index.html`:内联 boot script,逻辑与 `resolveTheme()` 保持同步
- `src/components/ThemeToggle.tsx`:循环按钮,挂在 `App.tsx` 的 `.header-right`
- `styles.css`:`:root` 加 `color-scheme: light`;dark 块 `@media` → `html[data-theme="dark"]`,`:root` 令牌规则与组件覆盖统一加前缀;新增 `.theme-toggle` 样式
- `src/i18n.tsx`:`theme_auto` / `theme_light` / `theme_dark` × 5 语言
