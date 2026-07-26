# 002 — GenAsset 语义角色 + duration 类型化(Finding 2)

## 背景

seam-findings Finding 2: Suno 的 `duration_secs`/`cover_url` 是一等产品输出,却塞进
`diagnostic_metadata: Value` 垃圾袋,因为类型化表面无处安放。诊断垃圾袋应只装真诊断数据
(如对齐置信度 `hootCer`)。与 001 合并设计、分开提交。

## 目标/范围

1. **GenAsset 角色**: `GenAsset` 两个 variant 增加 `role: GenAssetRole`(`Primary`/`Cover`/`Preview`;
   serde `default = Primary`,老 payload 无 role 字段仍可反序列化)。全部构造点机械更新(编译器找齐,
   含四个音乐 provider、五个视觉 provider、provider-core)。
2. **duration 类型化**: `GenResult` 增加 `duration_secs: Option<f64>`(时基媒体中性:music/video 填,
   image 留 None;`skip_serializing_if`)。
3. **suno 填充**: audio asset 标 `Primary`;`cover_url` 成为 `role: Cover` 的 Url asset;`duration_secs`
   提升到类型化字段;`diagnostic_metadata` 只留真诊断(`hootCer` 等),移除这两个 key。
4. **demo 采用**: `music_gen.rs` 改读 typed 字段与 asset 角色,删除扒 `diagnostic_metadata` 的
   `duration_secs`/`cover_url` 路径(同一 workspace 同步切换,不留双路径)。

## 验收标准

- [ ] `GenAsset` 带 `role`,serde 向后兼容(老 payload → Primary)
- [ ] `GenResult.duration_secs` 类型化;image-only provider 行为不变(None)
- [ ] suno 结果:cover 以 `role: Cover` asset 出现,duration 在 typed 字段,`diagnostic_metadata` 无此二 key
- [ ] demo 消费 typed 路径,`diagnostic_metadata` 的产品数据读取点清除(真诊断保留)
- [ ] 测试:老 payload serde 兼容、suno fetch 填充与角色标注、全 workspace 构造点编译通过
- [ ] 五项检查全绿

## 备注

- timed-text 不入 role 枚举:`timed_text` 已是 `GenResult` 一等字段(Finding 1 已落地)。
- 多 track variant 的 asset 分组(Finding 1 提到的另一缺口)不在本 issue。
