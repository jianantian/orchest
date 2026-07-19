# 003 — 注册容错 + 一致性小修

## 背景

- **注册连坐**: `register_skills` 任一条目失败(脚本 canonicalize 失败、工具重名、白名单外)→ 整个 run `fail_pre_start` 发 `RunFailed`(`crates/orchest/src/run/actor.rs:222-244`)。一个坏 skill 拖垮整个 run(SDK-D3)。
- **一致性小修**(SDK-D7): ① 小写 `skill.md` 能被发现(`scanner.rs:93`)但 `run/skills.rs:58` 只登记大写 SKILL.md 的遥测 → 小写 skill 失去 SkillContentRead 遥测;② skill 重名无检测(只有 bundled tool 名冲突才报错);③ 脚本 stderr 直接 `eprintln!` 进宿主进程(`bundled_tool.rs:288,341`),非结构化、可能泄漏。

## 目标/范围

1. **容错**: 单个 skill 注册失败 → 跳过该 skill + 结构化警告(复用 001 的警告通道),run 正常启动;builder 保留严格模式开关(任何失败即 RunFailed,默认关?——默认容错,strict 为 opt-in)。
2. 小写 `skill.md` 的遥测登记与大写一致。
3. skill 重名 → 警告(后者跳过或按确定性规则处理,文档写明)。
4. 脚本 stderr 接入 tracing(span 上下文),不再 `eprintln!`。

## 验收标准

- [ ] 一个坏 skill + 一个正常 skill 并存:run 正常启动,正常 skill 可用,坏 skill 有含路径与原因的警告
- [ ] 严格模式开启时:任一失败即 RunFailed(原行为)
- [ ] 小写 skill.md 的 SkillContentRead 遥测与大写一致(测试)
- [ ] 两个同名 skill:产生重名警告,行为确定(文档写明)
- [ ] 脚本 stderr 进 tracing(不进宿主 stderr)
- [ ] 五项检查全绿

## 备注

- 默认容错的取舍:与"失败可见"目标一致——警告永远在,run 不再连坐;strict 留给 CI/调试场景。
