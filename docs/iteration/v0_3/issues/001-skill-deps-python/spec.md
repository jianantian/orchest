# 001 · Skill 依赖管理：Python venv 隔离

## 背景

Skill 的 Python 脚本通常依赖第三方库（如 `requests`、`beautifulsoup4`），目前需要用户手动全局安装，体验差且容易冲突。用 `venv` 为每个 skill 创建隔离环境，runtime 自动管理安装和缓存。

## 目标

当 skill 的 `SKILL.md` 声明了 Python 依赖时，runtime 在首次执行该 skill 脚本前自动创建 venv 并安装依赖，后续复用缓存环境。

## 验收标准

**SKILL.md 格式：**
- [ ] frontmatter 支持 `dependencies.python: [...]` 字段（list of pip requirement strings）

示例：
```yaml
dependencies:
  python:
    - requests>=2.31
    - beautifulsoup4
```

**环境准备：**
- [ ] `SkillEnvManager::ensure_python_env(manifest: &SkillManifest) -> Result<PathBuf, EnvError>`
- [ ] 环境目录：`{cache_dir}/skill-envs/{skill_name}-{deps_hash}/`
- [ ] `deps_hash`：对 `dependencies.python` 列表排序后 SHA256 的前 8 位
- [ ] 环境不存在时：`python3 -m venv {env_dir}` + `{env_dir}/bin/pip install {deps...}`
- [ ] 环境已存在时直接返回路径（不重新安装）
- [ ] 安装失败时发出 `SkillDependencyError { skill_name, error }` 事件，返回 `Err`

**脚本执行集成：**
- [ ] `SkillBundledTool` 在 `executable = "python"` 时，使用 venv 内的 `python` 解释器（而非全局 python）
- [ ] 若 skill 无 `dependencies.python` 声明，行为与 v0.1 一致（使用全局 python）

**缓存目录：**
- [ ] 默认 `~/.cache/orchest/skill-envs/`，可通过 `ORCHEST_CACHE_DIR` 环境变量覆盖

## 说明

不支持 `pyproject.toml` 或 `requirements.txt` 文件——只支持 frontmatter 内联声明，保持 skill 的自包含性。
