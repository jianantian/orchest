# 004 · 实施计划

## 先读

- `crates/orchest-provider/src/{facade,registry}.rs`、`crates/orchest-{py,node}/src/asr_stream.rs`、两端公开类型及 SDK CI。

## 修改范围

- 两端 TTS 导出与类型、fixture、SDK 安装测试和配套指南。

## 步骤

1. 对照现有 ASR 入口，确定三种合成操作和流资源的两端公开形状。
2. 绑定一次性、整段流式与双工，使用原生字节类型、有界缓冲和按需交付。用同一组 fixture 测音频、控制、计量和各方言的不支持路径，记录首块延迟、吞吐和慢消费者下的缓冲峰值。
3. 回归 ASR 行为，跑三平台安装测试，记录测试与 SDK 对应的 Rust 版本。

## 安装包与流式测量

macOS arm64 使用 Rust 1.99.1、CPython 3.14.8 和 Node 22.22.2。发布模式构建的 wheel 和 npm
主包、平台包安装到仓库外的临时环境后通过全量 SDK 测试（Python 37 项、Node 26 项，含测量用例）。
Linux x86_64 和 arm64 通过 SDK packages 工作流独立构建并验收。

三个运行时使用同一个本地 WSS fixture。每次从打开流开始计时，接收 128 个 4096 字节的音频分片，共
512 KiB。正常读取和首块后暂停 100 ms 各测三次。吞吐为整个调用耗时内交付的音频量，包含本地 TLS
握手和 fixture 的协议处理。

| 运行时 | 正常首块（ms） | 正常总耗时（ms） | 正常吞吐（MiB/s） | 暂停后总耗时（ms） |
| --- | ---: | ---: | ---: | ---: |
| Rust | 2.06–9.30 | 3.76–11.10 | 45.0–133.0 | 105.61–109.73 |
| Python | 2.28–3.33 | 7.53–8.63 | 57.9–66.4 | 108.84–109.53 |
| Node | 3.29–3.54 | 10.64–10.90 | 45.9–47.0 | 105.55–108.33 |

Rust provider 的队列在暂停时峰值为 64 个事件。两个 SDK 都逐次从同一 Rust 队列读取，没有第二个音频队列。
双工 fixture 中，Python 的取消确认、正常关闭释放连接、异常退出释放连接分别为 0.15、0.30、0.15 ms，
Node 分别为 0.17、0.22、0.15 ms。这些数字描述本地 fixture 的开销。

SDK 测量使用 `ORCHEST_TTS_BENCHMARK=1` 运行 TTS 测试。Rust 测量将 fixture 启动时输出的 JSON
传入 `ORCHEST_TTS_FIXTURE`，然后执行 `cargo test --release -p orchest-provider --features tts --test tts_sdk_performance -- --ignored --nocapture`。
