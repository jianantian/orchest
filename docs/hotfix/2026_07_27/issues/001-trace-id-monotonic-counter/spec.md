# 001 — trace id 生成混入单调计数器(gh #240)

## 背景

`new_trace_id()`(`crates/orchest-provider-core/src/telemetry.rs:57-66`):`SystemTime` 纳秒 XOR
栈地址。macOS 时钟为 µs 粒度,同一调用点的栈地址相同 → 连续两次调用可产生相同 id。测试
`trace_ids_are_distinct_hex`(:83-89)在负载下偶发失败(v0.15 期间 4 次全量跑挂 2 次);生产上
trace id 碰撞意味着事件关联断裂。既是测试 flake,也是生产缺陷。

## 目标/范围

生成器混入进程级单调计数器(`AtomicU64`,`Relaxed` 即可):即使 nanos 与 salt 完全相同,序列号
也使每次调用产出不同 id。保持 32 位 hex 形态与"无外部 uuid 依赖"约束;注释更新为新的熵来源说明。

## 验收标准

- [x] 连续 N 次(如 10_000)调用 `new_trace_id()` 全不重复(新增测试,含相同栈地址场景)
- [x] 输出仍为 32 位 hex(现有断言不回归)
- [x] 原 flake 测试在高频复跑下稳定(如 `-- --exact trace_ids_are_distinct_hex` × 50)
- [x] 五项检查全绿

## 实施要点(hotfix 内嵌 plan)

- 读: `crates/orchest-provider-core/src/telemetry.rs`(:55-89 全貌)
- 改: `new_trace_id()` 加 `static SEQ: AtomicU64`;混合方式任选(如 `(nanos ^ salt).wrapping_add(seq)`
  或分段拼装),doc 注明熵来源 = 时钟 + 栈地址 + 进程内单调序列
- 测: 新增 uniqueness 压力测试(10_000 次进 HashSet);原测试保留
