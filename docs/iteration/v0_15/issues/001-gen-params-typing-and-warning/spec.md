# 001 — GenRequest 音乐参数类型化 + 未知 key 警告

## 背景

`GenRequest.params: Value` 无 schema(`crates/orchest-protocol/src/capability.rs:64-68`)。Suno
`PASSTHROUGH_PARAMS` 白名单(`crates/orchest-provider-http/src/gen/suno.rs:53-63`)之外的 key
静默丢弃——demo `build_gen_params`(`examples/demo/music-gift/src/tools/music_gen.rs:228`)传的
genre/tempo/mood/vocal_style/instrumentation/production/exclude 全丢,`vocalGender`/`negativeTags`/
`instrumental` 无人填,零报错;第三次 LLM 调用(music prompt 改写)产出 100% 空转。

SDK 优化计划 B1 两步走(先 warning 后类型化)在本 issue 一次做完:warning 是小步,与类型化同提交。

## 目标/范围

1. **类型化**: `orchest-protocol` 新增 `MusicParams`(音乐模态旋钮:`lyrics`/`instrumental`/`style`/
   `title`/`negative_tags`/`vocal_gender`/`style_weight`/`weirdness_constraint`/`audio_weight`),
   `GenRequest` 增加 `music: Option<MusicParams>`(`#[serde(default)]` + `skip_serializing_if`,
   wire 兼容;字段 serde camelCase,对齐 provider wire 惯例如 `negativeTags`)。`vocal_gender`
   用类型化 enum 还是 String,按 Suno 取值集合(m/f)裁定并在 doc 注明。
2. **provider 消费**: suno `build_submit_body` 优先读 typed 字段,raw `params` 仅作 dialect 特有
   补充(`personaId`/`personaModel`);其他音乐 provider(mureka/minimax_music/aliyun_music)按各自
   支持面把旋钮接上 typed(不支持的留 `params`,不强行映射)。
3. **未知 key 警告**: provider-core 提供共享 helper(如 `warn_unconsumed_params`),gen provider
   submit 时对"既非 typed 消费、也非显式处理、也非白名单"的 params key 打 `tracing::warn!`
   (低基数;key 名进 span field,不进 metric label)。所有按 key 挑拣 params 的 gen provider
   都要接,提交前列出消费点清单。
4. **demo 采用**: `build_gen_params` 改为构造 typed `MusicParams`(genre/tempo/mood/instrumentation/
   production 映射进 `style` 字符串;`exclude` → `negative_tags`;`vocal_style` → `vocal_gender`);
   误拼/未知 key 编译期或运行期显式报错。

## 验收标准

- [x] `MusicParams` 在 orchest-protocol 定义,serde 兼容(Py/Node wire 透传不受影响)
- [x] demo 提交路径全部走 typed;`vocalGender`/`instrumental`/`negativeTags` 有类型化入口
- [x] 构造含未知 key 的 params 提交 gen provider,tracing warn 可见 key 名
- [x] 未知 key 警告覆盖所有按 key 挑拣的 gen provider(spec/plan 或代码注释中列出清单)
- [x] 测试:typed 字段优先于 raw params;`build_submit_body` typed/raw 组合;警告 helper 单测
- [x] 五项检查全绿

## 备注

- 其他模态(image/video)参数类型化不在本 issue;`params` 保留为 dialect 逃生舱。
- Finding 2 的 GenAsset 角色/duration 提升在 002,两 issue 合并设计、分开提交。
