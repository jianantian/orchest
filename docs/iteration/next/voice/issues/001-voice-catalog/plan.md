# 001 · 实施计划

## 先读

- `docs/adr/0006-voice-session-model.md`、`crates/orchest-protocol/src/descriptor.rs`、`crates/orchest-provider-core/src/catalog.rs`、`crates/orchest-provider/src/{catalog,registry,facade}.rs`。
- `crates/orchest-provider-stream/src/catalog/` 与 ASR 契约 fixture。

## 修改范围

- 协议能力描述、provider-core 目录类型和筛选、流式实现目录、registry 校验测试。

## 步骤

1. 对照 ADR 与各方言 fixture 填写能力矩阵，标出待验证项。
2. 把能力投影到既有目录与 registry 查询入口，接入 TTS 模型记录。
3. 在选项解析位置加入校验，并用测试 transport 保证失败发生在网络连接之前。
