# 001 — 布局宽度与滚动模型统一

## 背景

`#root` 是 520px 定高(100dvh)框架,`.app-main` 是主滚动区;创作室用 `#root:has(.studio)` 把框架撑到 1080px,切 tab 时顶栏和框架宽度突变。滚动模型三种并存:页面屏走 `.app-main` 单滚动(playlist/mine)、guided 聊天内部滚动(输入框钉底)、创作室两栏各自独立滚动(`.studio-cols` 内每列 `min-height: 0` + overflow,同屏两个滚动条)。

## 目标/范围

只动 `examples/demo/music-gift/frontend/src/styles.css` 为主,组件 class 微调为辅。

1. **框架统一宽版**(桌面 ≥900px):`#root` 恒为 `max-width: 1080px`,删除 `#root:has(.studio)` 特例;<900px 保持 520 现状不变。
2. **各页内容列自居中**:
   - guided `.chat-panel`:`max-width: 520px; margin: 0 auto; width: 100%`;
   - playlist / mine / set-password / reset-password 等页面内容:`max-width: 600px` 居中(用现有页面 class,不新增抽象);空态/列表都在该列内;
   - studio 双栏占满框架宽;gift 页不动(本来就填满框架)。
3. **滚动模型统一为"每屏一个滚动条"**:
   - studio 放弃两栏独立滚动:`.studio-cols` 及两列去掉 `min-height: 0`/overflow 约束,内容自然生长,由 `.app-main` 统一滚动;
   - studio AI 面板的输入框用 `position: sticky; bottom: 0`(在 `.app-main` 滚动时保持可见),与 guided"输入框钉底"的心智一致;
   - guided 保持聊天列表内部单滚动(已是单滚动条,不动);
   - 移动端 studio tab 切换行为不变,同样改由 `.app-main` 滚动。
4. **顶栏稳定**:任何页面/任何 tab 切换,`.app-header` 与 `#root` 框架宽度不变(验证无 layout shift)。

## 验收标准

- [ ] 桌面(≥900px)guided ↔ studio 切换:`#root` 与顶栏宽度不变;guided 聊天列在宽框架中居中 520px
- [ ] playlist / mine 页面内容居中限宽,不顶满 1080px
- [ ] studio 双栏时页面只有 `.app-main` 一个滚动条;AI 对话增多后滚动页面,输入框吸附底部可见
- [ ] studio 草稿列很长时同样是单滚动条,无列内独立滚动
- [ ] 移动端(<900px)所有页面行为与现状一致
- [ ] 浅色/深色下各页无破版(令牌不变)
- [ ] `npm run build` 通过

## Notes

- gift 页在宽框架下的 unwrap 动画居中性顺手确认一眼即可,不调样式。
- 双栏在桌面保持同屏;AI 列内容少时与草稿列顶对齐即可。

## 实施步骤(plan)

读:

- `frontend/src/styles.css`:`#root`(87-98)、`.app-main`(263-272)、`.create-page`/`.playlist-page`(410+)、`.chat-panel`/`.chat-messages`(1005+)、studio 段(1660-1710)、`@media (max-width: 899px)` 相关块
- `frontend/src/pages/PlaylistPage.tsx`、`MyGiftsPage.tsx`、`SetPasswordPage.tsx`、`ResetPasswordPage.tsx` 的最外层 class
- `frontend/src/components/Studio.tsx` 的 `.studio-cols` 结构

改:

1. `styles.css`:
   - `#root` 加媒体查询:≥900px `max-width: 1080px`;删 `#root:has(.studio)`;
   - `.chat-panel` 桌面居中 520;
   - playlist/mine/auth 页面外层居中 600;
   - studio 段:去掉列级 overflow/min-height 约束,`.chat-bar`(studio 内)改 `position: sticky; bottom: 0;` + 背景色(继承框架 bg,避免滚动穿透);移动端镜像处理。
2. 组件仅在 class 名需要配合时微调(尽量纯 CSS)。
3. `npm run build`;agent-browser 桌面截图 guided/studio/playlist/mine 四页对比 + 一页深色抽验。
