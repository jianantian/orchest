# 004 · 实施计划

## 先读

- `crates/orchest-provider/src/{facade,registry}.rs`、`crates/orchest-{py,node}/src/asr_stream.rs`、两端公开类型及 SDK CI。

## 修改范围

- 两端 TTS 导出与类型、fixture、SDK 安装测试和配套指南。

## 步骤

1. 对照现有 ASR 入口，确定三种合成操作和流资源的两端公开形状。
2. 绑定一次性、整段流式与双工，使用原生字节类型、有界缓冲和按需交付。用同一组 fixture 测音频、控制、计量和各方言的不支持路径，记录首块延迟、吞吐和慢消费者下的缓冲峰值。
3. 回归 ASR 行为，跑三平台安装测试，记录测试与 SDK 对应的 Rust 版本。
