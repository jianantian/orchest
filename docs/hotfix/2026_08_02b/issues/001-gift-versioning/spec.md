# 001 — 后端:gift 版本化 + 编辑/再生成/版本查询端点

## 背景

生成的 gift 一旦创建就完全锁死:`lyrics` 列与 `meta`(title/style/vocal)没有任何更新端点,`POST /api/generate/{id}` 对 `done` 状态幂等返回不再提交(`tools/music_gen.rs:123`)。二次编辑需要:更新作品字段的端点、强制再生成的路径,以及按产品决策的 **gift 内版本化**(再生成产生新版本,旧版本保留可查,列表不膨胀,默认最新版)。

## 目标/范围

在 `examples/demo/music-gift/src/` 内:

1. **版本表** `gift_versions`:`(gift_id, version)` 主键,快照 `lyrics / meta / audio_url / cover_url / lrc / duration_secs / gen_request / created_at`。每次生成**成功完成**时写入一行(版本号 = 该 gift 现有最大版本 + 1,首版为 1)。`gifts` 行保持镜像最新版,现有所有读取(播放列表、公开作品页)不变。
2. **`PATCH /api/gift/{id}`**(creator-only,走 `verify_creator`):body `{ lyrics?, title?, style?, vocal? }`,更新 `lyrics` 列与 `meta` 的 `title/style/vocal` 键(未传的字段不动)。纯编辑,不触发生成。返回更新后的 gift。
3. **`POST /api/gift/{id}/regenerate`**(creator-only):
   - `gen_status` 为 `pending`/`running` 时返回 409(有任务在飞,拒绝并发再生成);
   - 否则重置生成态(`gen_status/gen_handle/audio_url/cover_url/lrc/duration_secs` 置空),然后调用现有 `music_gen::generate()` 走完好的提交流程(歌词为空等非 instrumental 情况由现有校验拒绝);
   - 旧音频文件**不删**(被旧版本行引用)。
4. **`GET /api/gift/{id}/versions`**(creator-only):版本列表按版本号降序,字段含 `version/lyrics/meta/audio_url/cover_url/lrc/duration_secs/created_at`,**不含** `gen_request`(调试列,礼物按 id 公开可见,不能进 API 响应——与 `Gift::gen_request` 同规)。无版本行的存量 gift 由 gift 行合成 v1 返回(不做数据回填迁移)。
5. **删除清理**:`delete_gift` 除现有音频外,一并清理所有版本行引用的音频文件(沿用现有的文件名安全校验:非空、无 `..`、无 `/`)。

## 验收标准

- [ ] 生成完成后 `gift_versions` 出现 v1 快照(歌词/meta/音频/封面/LRC 与 gift 行一致)
- [ ] PATCH 只传 `title` 时只改 meta.title,歌词/风格/人声不变;传 `lyrics` 时歌词列更新;非 creator(无 token 无 session)返回 403
- [ ] regenerate:重置生成态并成功重新提交;`pending`/`running` 时返回 409;非 creator 返回 403
- [ ] 再生成完成后产生 v2,gift 行镜像 v2,GET versions 返回 [v2, v1],v1 音频仍可访问
- [ ] GET versions 响应序列化中无 `gen_request` 字段;存量无版本行 gift 返回合成 v1
- [ ] delete_gift 后所有版本音频文件被清理
- [ ] `cargo test`(demo 包)通过,新逻辑有单元测试(版本写入、合成 v1、PATCH 部分更新、regenerate 409/重置)

## Notes

- 生日倒计时页内嵌歌词片段是创建时快照,不随再生成更新(已知限制,PRD 范围裁定)。
- 再生成期间 `audio_url` 清空,作品暂时离开播放列表(`list_published` 按 `audio_url.is_some()` 过滤),接受。

## 实施步骤(plan)

读:

- `src/gift.rs`(Gift/GiftStore,`update_audio`/`update_cover`/`update_lrc`/`set_gen_request` 的既有模式)
- `src/tools/music_gen.rs:560-630`(生成完成落库路径)
- `src/routes.rs`(`verify_creator`、`delete_gift`、`generate_music`)

改:

1. `src/gift.rs`:
   - `GiftVersion` 结构(序列化时跳过 `gen_request`);
   - `GiftStore::open` 建 `gift_versions` 表;
   - `add_version(&self, gift_id) -> AppResult<i64>`:从 gift 行快照插入,返回新版本号;
   - `list_versions(&self, gift_id) -> AppResult<Vec<GiftVersion>>`:降序;空表时由 gift 行合成 v1(仅当 gift 有 `audio_url` 时才合成——从未生成成功的 gift 没有版本);
   - `update_fields(&self, id, lyrics: Option<&str>, title/style/vocal: Option<&str>)`:部分更新 lyrics 列 + meta 键;
   - `reset_for_regeneration(&self, id)`:清 `gen_status/gen_handle/audio_url/cover_url/lrc/duration_secs`;
   - `version_audio_files(&self, id) -> Vec<String>`:供删除清理;
   - 单元测试。
2. `src/tools/music_gen.rs`:生成成功路径(`update_audio`/`update_cover`/`update_lrc` 之后)调 `add_version`,失败仅记日志不影响主流程。
3. `src/routes.rs`:
   - `PATCH /api/gift/{id}` handler(`update_gift`);
   - `POST /api/gift/{id}/regenerate` handler:409 守卫 → `reset_for_regeneration` → 复用 `music_gen::generate`;
   - `GET /api/gift/{id}/versions` handler;
   - `delete_gift` 增加版本音频清理;
   - 路由注册(`.route("/gift/{id}", get(get_gift).delete(delete_gift).patch(update_gift))` 等)。
4. `cargo test -p <demo 包名>` + `cargo clippy -- -D warnings` + `cargo fmt --check`。
