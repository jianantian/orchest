# 001 · 语音能力目录与便携选项

## 背景

ASR 目录已有五个流式方言与多个批量模型。`AsrCatalogExt`、`TtsCatalogExt` 仍是空结构，TTS 静态模型目录尚未接入 provider wall。

## 目标与范围

落实 ADR-0006 D8 的相关语音能力、发现筛选与会话前校验。区分 provider 来源和目录描述来源，维持 provider wall 的边界。

## 验收标准

- [x] 无凭证时可列出当前 ASR 行和 Aliyun、Volcengine、MiniMax 的已验证 TTS 行。查找时可按能力过滤，如客户端定稿、说话人、流式或双工。
- [x] 在同一模型所属的目录行可读取能力、来源和方言，注册表的能力描述和记录的行为一致。`direct` / `gateway` 与 `CapabilitySource` 的含义独立。
- [x] 请求本模型未声明的便携选项时返回 `UnsupportedOption`。测试 transport 的建连次数仍为零，受支持选项进入既有适配器。
- [x] 既有明确支持的 ASR 选项和 provider/model 入口通过回归测试。TTS 的双工声明随 003 的方言证据启用。

## 便携选项

ASR / TTS 请求可通过 `options.require_capabilities` 传入字符串数组，名称与能力目录一致。要求未声明的能力时，在连接或发送请求前返回 `UnsupportedOption`，格式错误返回 `InvalidRequest`。该字段由 Orchest 消费，其余方言选项沿用既有行为。AssemblyAI 批量 ASR 的 `speaker` 要求启用上游说话人标签，有返回时将分段结果附在转写元数据里。

## 依赖

v1.2。
