# 003 — 实施计划

## 要读的文件

- `crates/orchest/src/run/skills.rs`(register_skills 全流程)
- `crates/orchest/src/run/actor.rs:222-244`(fail_pre_start 路径)
- `crates/orchest/src/skill/bundled_tool.rs:288,341`(stderr 现状)
- `crates/orchest/src/tool/builtin.rs:140-166`(SkillContentRead 遥测登记)

## 要改的文件

- `crates/orchest/src/run/skills.rs`(容错 + 重名检测 + 小写遥测)
- `crates/orchest/src/run/config.rs`(strict 开关)
- `crates/orchest/src/skill/bundled_tool.rs`(stderr → tracing)
- 测试

## 步骤

1. register_skills 改逐条容错:失败条目收集为警告(001 的通道),不中断;全部失败≠错误(空 registry 也可启动)。
2. builder 加 strict 开关(默认容错)。
3. 重名检测:同名 skill 警告 + 确定性处理(建议先注册者胜,后者跳过并警告)。
4. 小写 skill.md 遥测登记统一(用扫描实际命中的文件名,而非硬编码大写)。
5. stderr 改 tracing::warn!(带 skill/tool span)。
6. 测试 + 五项检查。
