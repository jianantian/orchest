# 002 — 实施计划

## 要读的文件

- `crates/orchest-protocol/src/capability.rs`(GenAsset/GenResult 定义;TimedText 的 serde 惯例参照)
- `examples/demo/music-gift/docs/seam-findings.md` Finding 2(设计约束)
- `crates/orchest-provider-http/src/gen/suno.rs`(fetch 中 cover_url/duration_secs 的现填充点)
- 全 workspace `GenAsset::` / `GenResult` 构造点(rg 找齐)
- `examples/demo/music-gift/src/tools/music_gen.rs`(diagnostic_metadata 消费点)

## 要改的文件

- `crates/orchest-protocol/src/capability.rs`(+`GenAssetRole`、variant 字段、`duration_secs`)
- 全部 GenAsset/GenResult 构造点(provider http/visual/core)
- `crates/orchest-provider-http/src/gen/suno.rs`(角色标注 + typed 填充 + 垃圾袋清理)
- `examples/demo/music-gift/src/tools/music_gen.rs`
- 测试

## 步骤

1. `GenAssetRole` 定义 + 两个 variant 加 `role`(serde default);`GenResult.duration_secs`。
2. 全 workspace 构造点机械更新(rg `GenAsset::` 找齐,编译器兜底)。
3. suno fetch:cover → `role: Cover` asset、duration → typed 字段、diagnostic_metadata 清理;补单测。
4. demo 改 typed 消费,删 diagnostic_metadata 产品数据读取;更新单测。
5. serde 兼容测试:无 role 的老 payload 反序列化为 Primary。
6. 五项检查。
