# 002 · 实施计划

## 先读

- `crates/orchest-protocol/src/{capability,stream}.rs`、`crates/orchest-provider-core/src/{pricing,catalog}.rs`、语音 adapter 的完成路径。

## 修改范围

- 语音用量协议、时长和字符计价、目录定价接线及计量 fixture。

## 步骤

1. 先固定计量来源、单位、缺项和终止口径，再选择与现有终止事件兼容的传递方式。
2. 复用现有 `Pricing::Duration` 与 `Cost`，加入按字符计价及可追踪的定价来源。
3. 对照已知输入、上游覆盖、缺项、取消、失败和重试用例核对结果。
