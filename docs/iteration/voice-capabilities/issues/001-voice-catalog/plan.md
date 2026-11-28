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

## 能力证据

| 模型或方言 | 本次声明 | 验证 |
| --- | --- | --- |
| Aliyun、Volcengine 流式 ASR | 服务端切句 | V1 契约 fixture 的整句 Stable 与对应 Turn |
| Deepgram、Soniox 流式 ASR | 服务端切句、Stable Append、客户端 Finalize | V1 契约 fixture，适配器覆盖原生定稿回应与自动 Turn |
| ElevenLabs 流式 ASR | 客户端 Finalize | V1 契约 fixture 的 commit 回应 |
| AssemblyAI 批量 ASR | 说话人标签 | 请求字段映射与分段元数据的离线测试 |
| Aliyun、Volcengine、MiniMax TTS | 整段流式 | 现有三个适配器的可控 transport 测试 |

流式 ASR 的真实服务 fixture 由 [#353](https://github.com/jianantian/orchest/issues/353) 追踪。TTS 的双工与 Flush 声明由 [#373](https://github.com/jianantian/orchest/issues/373) 验证后启用。
