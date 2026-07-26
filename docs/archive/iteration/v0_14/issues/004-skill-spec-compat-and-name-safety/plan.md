# 004 — 实施计划

## 要读的文件

- `crates/orchest/src/skill/scanner.rs`(RawFrontmatter 字段)
- `crates/orchest/src/skill/types.rs`(SkillManifest)
- `crates/orchest/src/skill/env_manager.rs:50-56`(缓存路径拼接)
- Anthropic Agent Skills 官方规范(name/description 规则细节)

## 要改的文件

- `crates/orchest/src/skill/scanner.rs`(alias + 校验)
- `crates/orchest/src/skill/env_manager.rs`(路径防护)
- 测试

## 步骤

1. serde alias:`allowed-tools`/`allowed_tools` 双收;如有其他连字符/下划线不一致字段一并处理。
2. name/description 校验函数(kebab-case、1-64、目录名一致、description ≤1024),违反走 001 警告通道。
3. env_manager:非法 name 拒绝/转义 + canonicalize 前缀检查。
4. 测试:alias 两种写法、各非法 name、目录名不一致、description 超长、`../` 逃逸。
5. 五项检查。
