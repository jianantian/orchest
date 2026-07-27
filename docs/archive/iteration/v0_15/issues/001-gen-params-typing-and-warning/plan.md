# 001 — 实施计划

## 要读的文件

- `crates/orchest-protocol/src/capability.rs`(GenRequest/GenResult 定义与 serde 惯例)
- `crates/orchest-provider-http/src/gen/suno.rs`(`build_submit_body` :77 起、PASSTHROUGH :53-63、现有单测)
- `crates/orchest-provider-http/src/gen/`(mureka/minimax_music/aliyun_music 的 params 消费点)
- `crates/orchest-provider-visual/src/`(视觉 provider 的 params 挑拣点,接 warning)
- `crates/orchest-provider-core/src/gen.rs`(共享 helper 落点)
- `examples/demo/music-gift/src/tools/music_gen.rs`(`build_gen_params` :228)

## 要改的文件

- `crates/orchest-protocol/src/capability.rs`(+`MusicParams`、`GenRequest.music`)
- `crates/orchest-provider-core/src/gen.rs`(warning helper)
- 各 gen provider submit 实现(http 四个音乐 + visual 挑拣点)
- `examples/demo/music-gift/src/tools/music_gen.rs`
- 测试

## 步骤

1. `MusicParams` 定义(serde camelCase、全 Option)+ `GenRequest.music` 可选字段;vocal_gender 类型裁定并 doc 注明。
2. provider-core `warn_unconsumed_params` helper + 全 workspace 搜索 gen provider params 挑拣点,列清单,逐一接入。
3. suno `build_submit_body` typed 优先、raw 兜底 dialect 特有 key;补 typed/raw 组合单测。
4. 其他音乐 provider 按支持面接 typed。
5. demo `build_gen_params` 改 typed 构造,更新其单测。
6. 五项检查。
