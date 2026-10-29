# 003 implementation plan

## Files to read

- `crates/orchest-provider-stream/src/transport.rs`（替换传输层以回放 fixture 的接缝）
- `crates/orchest-provider/src/fakes.rs`（现有离线 fake 的组织方式）

## Files to change

- `crates/orchest-provider-stream/tests/asr_contract.rs`：断言集 + 每方言一组用例
- `crates/orchest-provider-stream/tests/fixtures/asr/<dialect>/*.jsonl`：录制的上下行帧
- `scripts/record-asr-fixture.*`：录制脚本
- `.github/workflows/ci.yml`：若契约测试需单独 feature，则加入

## Steps

1. 设计 fixture 格式（按时间顺序的上行 / 下行帧，去掉鉴权头）。
2. 在传输层加回放接缝（测试专用），让方言从 fixture 读帧。
3. 写断言集，作为一个函数对任意方言的事件序列运行。
4. 用 owner 提供的 key 录五个方言；脱敏后入库。
5. CI 接入；跑 workspace 检查。
