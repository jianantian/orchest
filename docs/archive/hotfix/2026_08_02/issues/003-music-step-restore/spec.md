# Issue 003:生成完成状态跨页面恢复

## 背景

引导模式生成完成后,对话里出现"你的歌完成了!"卡片(MusicCard),点击跳转 `/gift/:id` 拆礼物。但返回对话页(路由切换导致 `GuidedFlow` 卸载重挂载)后,界面退回 ReviewCard 的"生成这首歌"按钮——生成状态只存在于 `useMusicGen` 的组件内 state,从未持久化;`useGuidedState` 恢复快照时还显式把 `step: "music"` 降级为 `"review"`(注释说明了这正是为了避免恢复出没有 giftId 的死状态)。

## 目标/范围

- 会话快照(`sessionStorage["moment_guided"]`)持久化 `giftId` 和生成状态(`generating`/`ready`/`error`)
- 重挂载时若有持久化的 `giftId`,`step: "music"` 可正常恢复,MusicCard 按持久化状态渲染,点击可再次打开礼物
- `useMusicGen` 新增 `resume(id, state)`:终态直接恢复;`generating` 先 `getGift` 查库内终态(后台轮询在客户端离开后仍会收尾),仍在进行才重开 SSE watch
- 旧快照缺少新字段时按 `null` 兜底;无 `giftId` 的 `music` 快照仍降级 `review`(保留原兜底)
- "重新开始"清空持久化的 giftId/状态

非目标:不给 GiftPage 加返回按钮;不改 FreeCreatePanel(自由创作无此状态机)。

## 验收标准

- [ ] 生成完成 → 打开礼物 → 返回对话页,显示"你的歌完成了!"卡片而非生成按钮
- [ ] 生成中途离开再返回:已完成的显示完成卡片;仍在生成的显示进度卡并继续监听;失败的显示失败卡可重试
- [ ] 点击"重新开始"后快照清除,不残留旧 giftId
- [ ] 旧版本快照(无 giftId 字段)加载不报错,行为同改前(降级 review)

## 实施要点

- `useGuidedState.ts`:`Saved` 增加 `giftId`/`musicState`,新增 `setGift(id)`(置 generating)、`setMusicState(terminal)` 两个 action,`reset()` 一并清理
- `useMusicGen.ts`:`resume()` 如上;`gen_status === "done"` → ready,`"failed"/"timeout"` → error,否则重开 `watchGeneration`
- `GuidedFlow.tsx`:mount 时按快照 `resume`;`handleReviewSubmit` 在 `gen.start` 成功后 `act.setGift(giftId)`;`gen.state` 进入终态时 effect 持久化
