# 003 — 实施计划

## 要读的文件

- `crates/orchest-provider/src/`(registry 构建模型的路径,找 ModelSpec 构造点)
- `crates/orchest-provider-http/src/protocol.rs:379` 附近、`anthropic/profile.rs:136` 附近(catalog 的 context_window 字段形状)
- `crates/orchest/src/run/config.rs:496-520`(ModelSpec 与 context_window_size 的消费方式)
- `crates/orchest/src/run/actor.rs` 的上下文预估校验(消费 context_window_size 的位置)

## 要改的文件

- registry/factory 构建模型的位置(回填逻辑)
- 测试

## 步骤

1. 定位 registry 从 catalog 构建 `ModelSpec` 的确切位置。
2. 回填:catalog 有值且 ModelSpec 未显式设置时写入;显式值优先。
3. rustdoc/注释写明回填规则与优先级。
4. 测试:回填(有 catalog 值)、不覆盖(显式值)、保持 None(catalog 无值)。
5. 四件套 + cargo doc。
