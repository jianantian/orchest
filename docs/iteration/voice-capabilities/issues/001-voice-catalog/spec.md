# 001 · 语音能力目录与便携选项

## 背景

ASR 目录已有五个流式方言与一个批量项。`AsrCatalogExt`、`TtsCatalogExt` 仍是空结构，TTS 静态模型目录尚未接入 provider wall。

## 目标与范围

落实 ADR-0006 D8 的相关语音能力、发现筛选与会话前校验。区分 provider 来源和目录描述来源，维持 provider wall 的边界。

## 验收标准

- [ ] 无凭证时可列出当前 ASR 行和 Aliyun、Volcengine、MiniMax 的已验证 TTS 行。查找时可按能力过滤，如客户端定稿、说话人、流式或双工。
- [ ] 在同一模型所属的目录行可读取能力、来源和方言，注册表的能力描述和记录的行为一致。`direct` / `gateway` 与 `CapabilitySource` 的含义独立。
- [ ] 请求本模型未声明的便携选项时返回 `UnsupportedOption`。测试 transport 的建连次数仍为零，受支持选项进入既有适配器。
- [ ] 既有明确支持的 ASR 选项和 provider/model 入口通过回归测试。TTS 的双工声明随 003 的方言证据启用。

## 依赖

v1.2。
