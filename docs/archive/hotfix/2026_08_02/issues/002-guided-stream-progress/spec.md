# Issue 002:引导模式流式进度指示补全

## 背景

引导(交互)模式最后一轮对话存在两段"死寂"时间,用户以为卡住:

1. 打字指示器(`typing-indicator`)的渲染条件是 `messages.filter(m => m.role === "assistant").length === 0`——只在第一轮显示。之后每轮发送后到首个 token 到达之间没有任何反馈。
2. 最后一轮歌词主体在 `<<<LYRICS>>>` 标记之后流式返回,气泡只显示标记之前的文本(`raw.split("<<<LYRICS>>>")[0]`),标记后的大量 token 全程无 UI;若标记前文本为空,气泡整个不渲染。直到后端的 Elevating/Reviewing 事件才出现"正在改稿润色歌词…"类标签。

另外,点击"生成这首歌"后到 `giftId` 返回之间只有一个静态文字气泡,没有动画。

## 目标/范围

- 等待当前轮首个可见文本期间,每一轮都显示打字指示器(不再仅限第一轮)
- `<<<LYRICS>>>` 标记到达后、质量阶段开始前,显示"正在写歌词…"标签指示(复用 `reviewing-indicator` 样式,5 语言)
- 生成前过渡气泡(`creating_gift`)改为带跳动点的动画指示

非目标:不改后端 SSE 事件流;不改打字机 reveal 逻辑。

## 验收标准

- [ ] 任意一轮发送用户消息后、首个 token 到达前,立即出现打字指示器(含 review 页的改词轮)
- [ ] 最后一轮歌词标记到达后,聊天气泡冻结期间出现"正在写歌词…"动画指示,直到 Elevating/Reviewing 标签接管
- [ ] 点击生成后、`giftId` 返回前,过渡气泡有跳动点动画而非静态文字
- [ ] 各指示器互斥不出现重影(打字 / 写歌词 / 质量阶段三态互斥)

## 实施要点

- `GuidedFlow.tsx` 渲染前计算:`lastTurnText`(最后一条 assistant 消息标记前文本)、`lyricsStreaming`(流式中且内容含标记)
- 打字指示器条件改为 `streaming && !stage && !lyricsStreaming && !lastTurnText`,去掉"无 assistant 消息"和 `step === "chat"` 限制
- 新增 `writing_lyrics` i18n key × 5 语言
- `creating_gift` 气泡改用 `reviewing-indicator` 结构(三个 typing-dot + 标签)
