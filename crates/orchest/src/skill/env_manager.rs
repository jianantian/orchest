//! Skill environment management: dependency installation and capability validation.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::types::{skill_name_violations, EnvError, SkillCapabilities, SkillManifest};

pub struct CapabilityValidator;

impl CapabilityValidator {
    pub fn execution_env(capabilities: Option<&SkillCapabilities>) -> HashMap<String, String> {
        let mut env = HashMap::new();
        let Some(capabilities) = capabilities else {
            return env;
        };
        for name in &capabilities.env {
            if let Ok(value) = std::env::var(name) {
                env.insert(name.clone(), value);
            }
        }
        env
    }

    pub fn missing_capabilities_warning(manifest: &SkillManifest) -> bool {
        manifest.capabilities.is_none() && manifest.path.join("scripts").is_dir()
    }
}

#[derive(Debug, Clone)]
pub struct SkillEnvManager {
    cache_dir: PathBuf,
}

impl SkillEnvManager {
    pub fn default_cache_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("ORCHEST_CACHE_DIR") {
            return PathBuf::from(dir);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".cache").join("orchest")
    }

    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Cache directory for the skill's Python env. Returns
    /// `INVALID_SKILL_NAME` instead of a path when `manifest.name` violates
    /// the Agent Skills naming rules — a hostile SKILL.md could otherwise
    /// shape the cache path (`../`, embedded separators).
    pub fn python_env_path(&self, manifest: &SkillManifest) -> Result<PathBuf, EnvError> {
        self.env_dir(
            &manifest.name,
            &python_deps_hash(&manifest.dependencies.python),
        )
    }

    /// Cache directory for the skill's Node env. Same name gate as
    /// [`SkillEnvManager::python_env_path`].
    pub fn node_env_path(&self, manifest: &SkillManifest) -> Result<PathBuf, EnvError> {
        self.env_dir(
            &manifest.name,
            &format!("node-{}", node_deps_hash(&manifest.dependencies.node)),
        )
    }

    /// Joins `skill-envs/{name}-{suffix}` after gating on the Agent Skills
    /// name rules. The rules already exclude every path separator, so a
    /// valid name cannot escape the cache directory; the post-join
    /// containment check is defense in depth against that invariant ever
    /// breaking.
    fn env_dir(&self, name: &str, suffix: &str) -> Result<PathBuf, EnvError> {
        let violations = skill_name_violations(name);
        if !violations.is_empty() {
            tracing::warn!(
                skill_name = %name,
                reason = %violations.join("; "),
                "refusing to build skill env for invalid skill name"
            );
            return Err(EnvError {
                message: format!(
                    "refusing to build skill env for invalid name '{name}': {}",
                    violations.join("; ")
                ),
                code: Some("INVALID_SKILL_NAME".into()),
            });
        }
        let base = self.cache_dir.join("skill-envs");
        let candidate = base.join(format!("{name}-{suffix}"));
        if candidate.parent() != Some(base.as_path()) {
            tracing::warn!(
                path = %candidate.display(),
                "skill env path escapes the skill-envs cache directory; skipping env build"
            );
            return Err(EnvError {
                message: format!(
                    "skill env path '{}' escapes the skill-envs cache directory",
                    candidate.display()
                ),
                code: Some("SKILL_ENV_PATH_ESCAPE".into()),
            });
        }
        Ok(candidate)
    }

    /// Re-anchors `env_dir` under the canonicalized `skill-envs` base
    /// (creating it when missing) and verifies the result stays inside it.
    /// The name gate in `env_dir` makes escape impossible; the canonicalize
    /// and prefix check here are the second line of defense, resolving any
    /// symlinks in the configured cache path before the env is built there.
    async fn contained_env_dir(&self, env_dir: &Path) -> Result<PathBuf, EnvError> {
        let base = self.cache_dir.join("skill-envs");
        tokio::fs::create_dir_all(&base)
            .await
            .map_err(env_io_error)?;
        let base_canonical = tokio::fs::canonicalize(&base).await.map_err(env_io_error)?;
        // `env_dir` came from `env_dir`, so its last component is a single
        // validated directory name; `file_name` also rejects a trailing `..`.
        let leaf = env_dir.file_name().ok_or_else(|| EnvError {
            message: format!(
                "skill env path '{}' escapes the skill-envs cache directory",
                env_dir.display()
            ),
            code: Some("SKILL_ENV_PATH_ESCAPE".into()),
        })?;
        let anchored = base_canonical.join(leaf);
        if !anchored.starts_with(&base_canonical) {
            tracing::warn!(
                path = %env_dir.display(),
                "skill env path escapes the skill-envs cache directory; skipping env build"
            );
            return Err(EnvError {
                message: format!(
                    "skill env path '{}' escapes the skill-envs cache directory",
                    env_dir.display()
                ),
                code: Some("SKILL_ENV_PATH_ESCAPE".into()),
            });
        }
        Ok(anchored)
    }

    /// Ensures the skill's Python env exists, returning its directory.
    /// Skills with spec-invalid names get no env: the error surfaces as
    /// `INVALID_SKILL_NAME` and nothing is created on disk.
    pub async fn ensure_python_env(&self, manifest: &SkillManifest) -> Result<PathBuf, EnvError> {
        let env_dir = self
            .contained_env_dir(&self.python_env_path(manifest)?)
            .await?;
        if env_dir.exists() {
            return Ok(env_dir);
        }
        tokio::fs::create_dir_all(env_dir.parent().unwrap_or(&self.cache_dir))
            .await
            .map_err(env_io_error)?;
        let status = Command::new("python3")
            .arg("-m")
            .arg("venv")
            .arg(&env_dir)
            .status()
            .await
            .map_err(env_io_error)?;
        if !status.success() {
            return Err(EnvError {
                message: format!("python venv creation failed with status {status}"),
                code: Some("PYTHON_VENV_FAILED".into()),
            });
        }
        if !manifest.dependencies.python.is_empty() {
            let status = Command::new(env_dir.join("bin").join("pip"))
                .arg("install")
                .args(&manifest.dependencies.python)
                .status()
                .await
                .map_err(env_io_error)?;
            if !status.success() {
                return Err(EnvError {
                    message: format!("pip install failed with status {status}"),
                    code: Some("PYTHON_DEPENDENCY_FAILED".into()),
                });
            }
        }
        install_python_orchest_sdk(&env_dir).await?;
        Ok(env_dir)
    }

    /// Ensures the skill's Node env exists, returning its directory. Skills
    /// with spec-invalid names get no env: the error surfaces as
    /// `INVALID_SKILL_NAME` and nothing is created on disk.
    pub async fn ensure_node_env(&self, manifest: &SkillManifest) -> Result<PathBuf, EnvError> {
        let env_dir = self
            .contained_env_dir(&self.node_env_path(manifest)?)
            .await?;
        if env_dir.join("node_modules").exists() {
            return Ok(env_dir);
        }
        tokio::fs::create_dir_all(&env_dir)
            .await
            .map_err(env_io_error)?;
        let package = serde_json::json!({ "dependencies": manifest.dependencies.node });
        tokio::fs::write(
            env_dir.join("package.json"),
            serde_json::to_vec_pretty(&package).map_err(|e| EnvError {
                message: format!("failed to serialize package.json: {e}"),
                code: Some("SERIALIZE_PACKAGE_JSON".into()),
            })?,
        )
        .await
        .map_err(env_io_error)?;
        let status = Command::new("npm")
            .arg("install")
            .arg("--prefix")
            .arg(&env_dir)
            .status()
            .await
            .map_err(env_io_error)?;
        if !status.success() {
            return Err(EnvError {
                message: format!("npm install failed with status {status}"),
                code: Some("NODE_DEPENDENCY_FAILED".into()),
            });
        }
        install_node_orchest_sdk(&self.cache_dir).await?;
        Ok(env_dir)
    }

    pub async fn ensure_builtin_node_sdk(&self) -> Result<PathBuf, EnvError> {
        install_node_orchest_sdk(&self.cache_dir).await?;
        Ok(self.cache_dir.join("orchest-sdk-node"))
    }
}

impl Default for SkillEnvManager {
    fn default() -> Self {
        Self::new(Self::default_cache_dir())
    }
}

fn env_io_error(error: std::io::Error) -> EnvError {
    EnvError {
        message: error.to_string(),
        code: Some("IO_ERROR".into()),
    }
}

fn python_deps_hash(deps: &[String]) -> String {
    let mut sorted = deps.to_vec();
    sorted.sort();
    let joined = sorted.join("\n");
    hash8(joined.as_bytes())
}

fn node_deps_hash(deps: &BTreeMap<String, String>) -> String {
    let bytes = serde_json::to_vec(deps).unwrap_or_default();
    hash8(&bytes)
}

fn hash8(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")[..8].to_string()
}

async fn install_python_orchest_sdk(env_dir: &Path) -> Result<(), EnvError> {
    let lib_dir = env_dir.join("lib");
    let mut entries = tokio::fs::read_dir(&lib_dir).await.map_err(env_io_error)?;
    while let Some(entry) = entries.next_entry().await.map_err(env_io_error)? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("python") {
            let package_dir = entry.path().join("site-packages").join("orchest_sdk");
            tokio::fs::create_dir_all(&package_dir)
                .await
                .map_err(env_io_error)?;
            tokio::fs::write(
                package_dir.join("__init__.py"),
                r#"
def create_sub_agent(parent_run_id=None, config=None, input=None):
    return {"error": "sub_agent_requests_require_agent_delegate_tool"}
"#,
            )
            .await
            .map_err(env_io_error)?;
            return Ok(());
        }
    }
    Err(EnvError {
        message: "failed to locate venv site-packages".into(),
        code: Some("PYTHON_SDK_INSTALL_FAILED".into()),
    })
}

async fn install_node_orchest_sdk(cache_dir: &Path) -> Result<(), EnvError> {
    let package_dir = cache_dir.join("orchest-sdk-node").join("orchest-sdk");
    tokio::fs::create_dir_all(&package_dir)
        .await
        .map_err(env_io_error)?;
    tokio::fs::write(
        package_dir.join("index.js"),
        r#"
function create_sub_agent(parentRunId, config, input) {
  return { error: "sub_agent_requests_require_agent_delegate_tool" };
}

module.exports = { create_sub_agent };
"#,
    )
    .await
    .map_err(env_io_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_named(name: &str) -> SkillManifest {
        SkillManifest {
            name: name.to_string(),
            description: "d".into(),
            path: PathBuf::from("/nonexistent").join(name),
            skill_md_path: PathBuf::new(),
            allowed_tools: None,
            bundled_tools: vec![],
            dependencies: Default::default(),
            capabilities: None,
            raw_frontmatter: serde_json::Value::Null,
        }
    }

    #[test]
    fn capability_validator_builds_declared_env_only() {
        std::env::set_var("ORCHEST_TEST_SECRET", "visible");
        std::env::set_var("ORCHEST_UNDECLARED_SECRET", "hidden");
        let capabilities = SkillCapabilities {
            network: false,
            filesystem_read: vec![],
            filesystem_write: vec![],
            env: vec!["ORCHEST_TEST_SECRET".into()],
            max_memory_mb: None,
        };

        let env = CapabilityValidator::execution_env(Some(&capabilities));

        assert_eq!(
            env.get("ORCHEST_TEST_SECRET").map(String::as_str),
            Some("visible")
        );
        assert!(!env.contains_key("ORCHEST_UNDECLARED_SECRET"));
    }

    #[test]
    fn env_path_refuses_spec_invalid_names() {
        let manager = SkillEnvManager::new(PathBuf::from("/nonexistent-cache"));
        // `../x` and `a/b` are the path-injection shapes; the others violate
        // the same name rules that keep separators out of the cache path.
        for name in ["../x", "a/b", "Bad-Name", "under_scored"] {
            let manifest = manifest_named(name);
            let err = manager
                .python_env_path(&manifest)
                .expect_err(&format!("python path accepted '{name}'"));
            assert_eq!(err.code.as_deref(), Some("INVALID_SKILL_NAME"));
            assert!(err.message.contains(name), "unexpected message: {err}");
            let err = manager
                .node_env_path(&manifest)
                .expect_err(&format!("node path accepted '{name}'"));
            assert_eq!(err.code.as_deref(), Some("INVALID_SKILL_NAME"));
        }
    }

    #[test]
    fn env_path_for_valid_name_stays_inside_skill_envs() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SkillEnvManager::new(tmp.path().to_path_buf());
        let manifest = manifest_named("good-skill");

        let python = manager.python_env_path(&manifest).unwrap();
        assert!(python.starts_with(tmp.path().join("skill-envs")));
        assert_eq!(
            python.parent(),
            Some(tmp.path().join("skill-envs").as_path())
        );

        let node = manager.node_env_path(&manifest).unwrap();
        assert!(node
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("good-skill-node-"));
    }

    #[tokio::test]
    async fn ensure_env_creates_nothing_for_spec_invalid_names() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = SkillEnvManager::new(tmp.path().to_path_buf());

        for name in ["../x", "a/b"] {
            let manifest = manifest_named(name);
            let err = manager.ensure_python_env(&manifest).await.unwrap_err();
            assert_eq!(err.code.as_deref(), Some("INVALID_SKILL_NAME"));
            let err = manager.ensure_node_env(&manifest).await.unwrap_err();
            assert_eq!(err.code.as_deref(), Some("INVALID_SKILL_NAME"));
        }

        // No env build means no directories anywhere: nothing escaped into
        // the cache root, and `skill-envs` was never created either.
        let mut entries = std::fs::read_dir(tmp.path()).unwrap();
        assert!(
            entries.next().is_none(),
            "cache root must stay empty, found: {:?}",
            entries.next()
        );
    }
}
