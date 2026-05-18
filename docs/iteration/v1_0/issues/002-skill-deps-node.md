# 002 · Skill 依赖管理：Node npm 隔离

## 背景

与 Python venv 类似，Node skill 脚本可能依赖 npm 包。每个 skill 使用独立的 `node_modules` 目录隔离依赖。

## 目标

当 skill 的 `SKILL.md` 声明了 Node 依赖时，runtime 在首次执行该 skill 脚本前自动安装 npm 包，后续复用缓存。

## 验收标准

**SKILL.md 格式：**
- [ ] frontmatter 支持 `dependencies.node: { ... }` 字段（等价于 `package.json` 的 `dependencies` 字段）

示例：
```yaml
dependencies:
  node:
    axios: "^1.6"
    cheerio: "^1.0"
```

**环境准备：**
- [ ] `SkillEnvManager::ensure_node_env(manifest: &SkillManifest) -> Result<PathBuf, EnvError>`
- [ ] 环境目录：`{cache_dir}/skill-envs/{skill_name}-node-{deps_hash}/`
- [ ] 在该目录生成 `package.json`（只含 `dependencies` 字段），然后运行 `npm install --prefix {env_dir}`
- [ ] `deps_hash`：对依赖 JSON 序列化后 SHA256 的前 8 位
- [ ] 安装失败时发出 `SkillDependencyError` 事件

**脚本执行集成：**
- [ ] `SkillBundledTool` 在 `executable = "node"` 时，设置 `NODE_PATH={env_dir}/node_modules` 环境变量
- [ ] 若 skill 无 `dependencies.node` 声明，行为与 v0.1 一致

## 说明

使用 `NODE_PATH` 而不是 `--require` 或修改脚本，保持对 skill 脚本的零侵入。Node 版本管理（nvm 等）不在范围内——由用户确保系统 `node` 可用。
