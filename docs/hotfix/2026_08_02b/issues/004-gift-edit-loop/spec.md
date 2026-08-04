# 004 — 前端:二次编辑闭环(编辑入口 + 版本切换)

## 背景

003 的创作室只有"新建"形态。本 issue 接通**二次编辑闭环**(产品决策):GiftPage 与 MyGifts 双入口进入创作室编辑已有作品;保存并重新生成(歌词/风格/人声变更时)在同一 gift 内产生新版本(001 的端点);标题是轻编辑只保存不再生成;旧版本可切换回听、可载入草稿再改。

## 目标/范围

在 `examples/demo/music-gift/frontend/src/` 内:

1. **编辑入口**:
   - GiftPage owner 操作区(沿用现有 owner 判定 `GiftPage.tsx:286`:creator_token 或 session 匹配)加"编辑"按钮 → `navigate("/create?edit={id}")`;
   - MyGifts 列表每张卡片加编辑入口 → 同上。
2. **Studio edit 模式**:`/create?edit={id}` 时 CreatePage 强制 studio tab;Studio 以 `editGiftId` 形态(003 组件 + 轻封装,取简)工作:
   - 载入 gift:`getGift(id)` 把 `lyrics / meta.style / meta.vocal / meta.title` 填入草稿;
   - 显示当前版本音频播放器(复用 `AudioPlayer`);
   - **保存**:只改标题 → 仅 `PATCH /api/gift/{id}`(meta.title),提示已保存;
   - **保存并重新生成**:歌词/风格/人声有变更 → PATCH 后 `POST /api/gift/{id}/regenerate`,复用 `watchGeneration` 展示生成中/完成/失败;完成后刷新 gift 与版本列表;
   - 生成中禁止再次提交;非 owner 打开 edit 链接时隐藏编辑能力(只读提示)。
3. **版本切换器**(edit 模式内):`GET /api/gift/{id}/versions`,>1 个版本时显示版本 pills(V2 最新、V1…,默认选中最新);切换即换音频/歌词/封面/LRC 的展示(纯前端切换);"载入到草稿"把该版本的歌词/风格/人声/标题填入编辑区,供基于旧版再改。
4. **GuidedFlow ReviewCard** 加"在创作室中打开":把当前 draft(lyrics/style/title/vocal)带入 studio 新建形态(sessionStorage 传递,与 003 的草稿持久化同通道),切到 free tab。
5. i18n:编辑/保存/重新生成/版本/载入到草稿/已保存等 key,en/zh。

## 验收标准

- [ ] GiftPage owner 看到"编辑"按钮并进入 `/create?edit={id}`;非 owner 看不到;MyGifts 卡片有编辑入口
- [ ] edit 模式载入后草稿与 gift 现状一致,当前音频可播放
- [ ] 只改标题 → 保存后 gift meta.title 更新,不触发生成(无新 gen 任务)
- [ ] 改歌词 → 保存并重新生成 → 生成完成后作品页是新音频,版本列表出现新版本且默认选中最新的
- [ ] 切到 V1:播放器与歌词展示 V1 内容;"载入到草稿"后编辑区填入 V1 字段
- [ ] `pending`/`running` 时再生成按钮禁用(或后端 409 有友好提示)
- [ ] ReviewCard"在创作室中打开"后 studio 草稿带上 guided 的 draft
- [ ] `npm run build`(tsc + vite)通过;agent-browser 冒烟编辑→再生成→版本切换路径

## Notes

- 再生成期间旧音频不可用(001 范围裁定),edit 模式 UI 展示生成中状态即可。
- 版本切换器只在 edit 模式出现;GiftPage 公开页始终展示最新版,不加版本 UI。
- 引导模式生成完成后的 MusicCard → 作品页 → 编辑 动线由此闭环;不在聊天里内嵌编辑。

## 实施步骤(plan)

读:

- `frontend/src/pages/GiftPage.tsx`(owner 判定、音频播放、生成状态 union)
- `frontend/src/pages/MyGiftsPage.tsx`
- `frontend/src/components/AudioPlayer.tsx`、`MusicCard.tsx`
- 003 产出的 `Studio.tsx`、`api.ts` 新端点封装

改:

1. `api.ts`:`updateGift(id, fields, token?)`(PATCH)、`regenerateGift(id, token?)`、`getGiftVersions(id, token?)`;`types.ts` 加 `GiftVersion`。
2. `App.tsx` 路由无需变(edit 经 query 参数,CreatePage 读 `useSearchParams`)。
3. `CreatePage.tsx`:`?edit={id}` 时强制 studio tab 并下传 `editGiftId`。
4. Studio edit 形态:载入 gift、播放器、保存/保存并重新生成、版本切换器(watchGeneration 复用 003 的 gen 状态展示)。
5. `GiftPage.tsx`:owner 区加编辑按钮;`MyGiftsPage.tsx`:卡片编辑入口。
6. `GuidedFlow.tsx` ReviewCard 区加"在创作室中打开"(写 sessionStorage → `onSwitchToFree`)。
7. `i18n.tsx`、`styles.css`(版本 pills、编辑按钮样式)。
8. `npm run build`;agent-browser 冒烟。
