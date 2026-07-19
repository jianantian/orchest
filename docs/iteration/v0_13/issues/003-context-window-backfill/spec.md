# 003 — catalog context_window 回填 ModelSpec

## 背景

catalog 里每个模型都有 `context_window`(`crates/orchest-provider-http/src/protocol.rs:379`、`anthropic/profile.rs:136` 等),但从不回填到运行时的 `ModelSpec.context_window_size`(默认 None,`crates/orchest/src/run/config.rs:504`;Py/Node 绑定硬编码 None)。后果:调用前上下文硬校验(`run/actor.rs` 的预估校验)与 compaction(默认关)双双失效——上下文无限增长直到 provider 400、run 死亡。这是 SDK-C3。

## 目标/范围

provider registry 构建模型时,把 catalog 的 `context_window` 回填到 `ModelSpec.context_window_size`;**用户显式设置的值优先**,catalog 无该字段时保持 None。不改 compaction 的默认开关(保持 opt-in);本 issue 只接数据通路。

## 验收标准

- [ ] 经 registry 创建且 catalog 有 `context_window` 的模型,`ModelSpec.context_window_size` 等于 catalog 值
- [ ] 使用方显式设置的 `context_window_size` 不被覆盖
- [ ] catalog 无 `context_window` 的模型保持 None(行为与现状一致)
- [ ] 测试:回填 / 不覆盖 / 无字段三分支
- [ ] 四件套 + cargo doc 全绿

## 备注

- 回填后,`run/actor.rs` 的调用前硬校验自然生效(其逻辑已存在,只是从未拿到非 None 值);属预期行为变化,在 commit body 写明。
- Py/Node 绑定硬编码 None 处同步受益,无需改绑定。
