# 004 — 标准兼容 + name 校验 + 路径注入防护

## 背景

- **标准字段静默忽略**(SDK-D4): Anthropic Agent Skills 官方规范的可选字段是 `allowed-tools`(连字符),scanner 只读 `allowed_tools`(下划线,无 serde alias,`crates/orchest/src/skill/scanner.rs:19`)→ 标准 skill 的字段被静默忽略。官方还规定 name 格式(kebab-case、1-64 字符、与目录名一致)与 description ≤1024,当前无校验。
- **路径注入**(SDK-D6): `skill/env_manager.rs:50-56` 用未校验的 `manifest.name` 拼 `skill-envs/{name}-{hash}`;name 含 `/`/`..` 可塑造缓存目录路径(source 是用户提供的 SKILL.md)。

## 目标/范围

1. `allowed-tools`(连字符)与 `allowed_tools`(下划线)都收(serde alias);官方 `license`/`compatibility`/`metadata` 字段如需要可存入 raw(不强制)。
2. name 校验:kebab-case、1-64 字符;与目录名不一致 → 警告(不阻断,001 通道)。description >1024 → 警告。
3. 路径注入防护:name 校验不通过时 env_manager 拒绝建目录(或转义);拼接后 canonicalize + 前缀检查,逃逸 → 警告 + 跳过 env 构建。

## 验收标准

- [ ] 官方示例 skill 的 `allowed-tools` 被正确解析(两种写法都有测试)
- [ ] 非法 name(大写/下划线/超长/与目录名不符)产生警告,skill 仍按确定性规则加载(文档写明)
- [ ] 恶意 name(`../x`、`a/b`)无法逃逸 skill-envs 目录(测试)
- [ ] description >1024 产生警告
- [ ] 五项检查全绿

## 备注

- 校验与 001 的警告通道复用;不引入"拒绝加载"的新失败模式(警告 + 确定性行为)。
