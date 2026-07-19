# 002 — MaxTokens 截断标记

## 背景

`StopReason::MaxTokens` 且无 tool_use 时,run 以 `RunCompleted` 正常结束,截断文本作为最终输出,无标记、无事件区分(hotfix 2026_07_18b 后位于 `crates/orchest/src/run/actor.rs` 的 EndTurn/MaxTokens 分支)。消费方(music-gift countdown、review pass)无法知道输出被砍断——countdown HTML 截断后以 ready 落盘(JS 全废),review pass 截断丢 `<<<END>>>` 静默回退。

## 目标/范围

让截断完成在事件层可区分:`RuntimeEvent::RunCompleted` 增加截断信息——推荐加 `stop_reason: StopReason` 字段(信息最全;或 `truncated: bool`,实现时二选一并说明理由)。要求:

- serde 向后兼容:新字段 `#[serde(default)]`,旧 JSON 可反序列化;
- Py/Node 事件映射透传该字段;
- rustdoc 写明消费方应据此决定续写/重试/报错;
- 不改两种完成路径的终止行为本身。

## 验收标准

- [ ] `MaxTokens` 完成时,事件携带的标记与 `EndTurn` 完成可区分
- [ ] 旧形状的事件 JSON 仍可反序列化(`serde(default)`)
- [ ] Py/Node 绑定透出该字段
- [ ] rustdoc 写明消费方语义
- [ ] 测试:两种 stop_reason 的事件断言 + serde 兼容断言
- [ ] 四件套 + cargo doc 全绿

## 备注

- demo countdown 的截断判定从"结尾校验兜底"升级为读标记,属 demo 侧后续(optimization-plan D4 注)。
