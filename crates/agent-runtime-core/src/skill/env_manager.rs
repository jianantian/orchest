use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::types::{EnvError, SkillCapabilities, SkillManifest};

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

    pub fn python_env_path(&self, manifest: &SkillManifest) -> PathBuf {
        self.cache_dir.join("skill-envs").join(format!(
            "{}-{}",
            manifest.name,
            python_deps_hash(&manifest.dependencies.python)
        ))
    }

    pub fn node_env_path(&self, manifest: &SkillManifest) -> PathBuf {
        self.cache_dir.join("skill-envs").join(format!(
            "{}-node-{}",
            manifest.name,
            node_deps_hash(&manifest.dependencies.node)
        ))
    }

    pub async fn ensure_python_env(&self, manifest: &SkillManifest) -> Result<PathBuf, EnvError> {
        let env_dir = self.python_env_path(manifest);
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

    pub async fn ensure_node_env(&self, manifest: &SkillManifest) -> Result<PathBuf, EnvError> {
        let env_dir = self.node_env_path(manifest);
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
    format!("{:x}", digest)[..8].to_string()
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
}
