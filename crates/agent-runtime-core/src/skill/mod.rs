pub mod bundled_tool;
pub mod executor;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::tool::{JsonSchema, ToolMetadata, ToolSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub allowed_tools: Option<Vec<String>>,
    pub bundled_tools: Vec<BundledToolDef>,
    #[serde(default)]
    pub dependencies: SkillDependencies,
    #[serde(default)]
    pub capabilities: Option<SkillCapabilities>,
    pub raw_frontmatter: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDependencies {
    #[serde(default)]
    pub python: Vec<String>,
    #[serde(default)]
    pub node: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillCapabilities {
    #[serde(default)]
    pub network: bool,
    #[serde(default, rename = "filesystem_read")]
    pub filesystem_read: Vec<PathBuf>,
    #[serde(default, rename = "filesystem_write")]
    pub filesystem_write: Vec<PathBuf>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct RawCapabilities {
    #[serde(default)]
    network: bool,
    #[serde(default)]
    filesystem: RawFilesystemCapabilities,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    max_memory_mb: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct RawFilesystemCapabilities {
    #[serde(default)]
    read: Vec<PathBuf>,
    #[serde(default)]
    write: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledToolDef {
    pub name: String,
    pub description: String,
    pub executable: String,
    pub script: PathBuf,
    pub input_schema: JsonSchema,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("failed to read directory: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct EnvError {
    pub message: String,
    pub code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    bundled_tools: Vec<RawBundledTool>,
    #[serde(default)]
    dependencies: SkillDependencies,
    #[serde(default)]
    capabilities: Option<RawCapabilities>,
}

#[derive(Debug, Deserialize)]
struct RawBundledTool {
    name: String,
    description: String,
    executable: String,
    script: String,
    #[serde(default)]
    input_schema: Value,
}

pub struct SkillScanner;

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

impl SkillScanner {
    pub fn scan(dir: &Path) -> Result<Vec<SkillManifest>, ScanError> {
        let mut manifests = Vec::new();

        if !dir.exists() {
            return Ok(manifests);
        }

        Self::scan_recursive(dir, &mut manifests);
        Ok(manifests)
    }

    fn scan_recursive(dir: &Path, manifests: &mut Vec<SkillManifest>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = Self::find_skill_md(&path);
                if let Some(md_path) = skill_md {
                    if let Some(manifest) = Self::parse_skill_md(&md_path, &path) {
                        manifests.push(manifest);
                    }
                } else {
                    Self::scan_recursive(&path, manifests);
                }
            }
        }
    }

    fn find_skill_md(dir: &Path) -> Option<PathBuf> {
        for name in &["SKILL.md", "skill.md"] {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    fn parse_skill_md(md_path: &Path, skill_dir: &Path) -> Option<SkillManifest> {
        let content = std::fs::read_to_string(md_path).ok()?;

        let frontmatter = Self::extract_frontmatter(&content)?;
        let raw: RawFrontmatter = serde_yaml::from_str(&frontmatter).ok()?;
        let raw_value: Value = serde_yaml::from_str(&frontmatter).ok()?;

        let abs_dir = std::fs::canonicalize(skill_dir).unwrap_or_else(|_| skill_dir.to_path_buf());

        let bundled_tools = raw
            .bundled_tools
            .into_iter()
            .map(|t| BundledToolDef {
                name: t.name,
                description: t.description,
                executable: t.executable,
                script: PathBuf::from(t.script),
                input_schema: t.input_schema,
            })
            .collect();
        let capabilities = raw.capabilities.map(|capabilities| SkillCapabilities {
            network: capabilities.network,
            filesystem_read: capabilities.filesystem.read,
            filesystem_write: capabilities.filesystem.write,
            env: capabilities.env,
            max_memory_mb: capabilities.max_memory_mb,
        });

        Some(SkillManifest {
            name: raw.name,
            description: raw.description,
            path: abs_dir,
            allowed_tools: raw.allowed_tools,
            bundled_tools,
            dependencies: raw.dependencies,
            capabilities,
            raw_frontmatter: raw_value,
        })
    }

    fn extract_frontmatter(content: &str) -> Option<String> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return None;
        }

        let after_first = &trimmed[3..];
        let end = after_first.find("---")?;
        Some(after_first[..end].to_string())
    }
}

impl SkillManifest {
    pub fn build_tool_metadata(&self, _tool: &BundledToolDef) -> ToolMetadata {
        ToolMetadata {
            side_effect: false,
            requires_approval: false,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::Skill {
                skill_name: self.name.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_skill(dir: &Path) {
        let skill_dir = dir.join("test_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: test_skill
description: A test skill for unit testing
allowed_tools:
  - read_file
bundled_tools:
  - name: greet
    description: Greets someone
    executable: python
    script: scripts/greet.py
    input_schema:
      type: object
      properties:
        name:
          type: string
      required:
        - name
---

# Test Skill

This is a test skill.
"#,
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/greet.py"), "# placeholder").unwrap();
    }

    fn create_v03_skill(dir: &Path) {
        let skill_dir = dir.join("v03_skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: v03_skill
description: A v0.3 skill
dependencies:
  python:
    - requests>=2.31
    - beautifulsoup4
  node:
    axios: "^1.6"
capabilities:
  network: true
  filesystem:
    read:
      - data
    write:
      - out
  env:
    - ORCHEST_TEST_SECRET
  max_memory_mb: 256
bundled_tools:
  - name: run
    description: Runs
    executable: python
    script: scripts/run.py
---

# v0.3 Skill
"#,
        )
        .unwrap();
        fs::write(skill_dir.join("scripts/run.py"), "print({})").unwrap();
    }

    #[test]
    fn scan_finds_skills() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill(tmp.path());

        let manifests = SkillScanner::scan(tmp.path()).unwrap();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].name, "test_skill");
        assert_eq!(manifests[0].description, "A test skill for unit testing");
        assert_eq!(manifests[0].allowed_tools, Some(vec!["read_file".into()]));
        assert_eq!(manifests[0].bundled_tools.len(), 1);
        assert_eq!(manifests[0].bundled_tools[0].name, "greet");
        assert_eq!(manifests[0].bundled_tools[0].executable, "python");
    }

    #[test]
    fn scan_parses_dependencies_and_capabilities() {
        let tmp = tempfile::tempdir().unwrap();
        create_v03_skill(tmp.path());

        let manifests = SkillScanner::scan(tmp.path()).unwrap();
        let manifest = &manifests[0];

        assert_eq!(
            manifest.dependencies.python,
            vec!["requests>=2.31".to_string(), "beautifulsoup4".to_string()]
        );
        assert_eq!(
            manifest.dependencies.node.get("axios").map(String::as_str),
            Some("^1.6")
        );

        let capabilities = manifest.capabilities.as_ref().unwrap();
        assert!(capabilities.network);
        assert_eq!(capabilities.filesystem_read, vec![PathBuf::from("data")]);
        assert_eq!(capabilities.filesystem_write, vec![PathBuf::from("out")]);
        assert_eq!(capabilities.env, vec!["ORCHEST_TEST_SECRET".to_string()]);
        assert_eq!(capabilities.max_memory_mb, Some(256));
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
    fn scan_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let manifests = SkillScanner::scan(tmp.path()).unwrap();
        assert!(manifests.is_empty());
    }

    #[test]
    fn scan_nonexistent_dir() {
        let manifests = SkillScanner::scan(Path::new("/nonexistent/path")).unwrap();
        assert!(manifests.is_empty());
    }

    #[test]
    fn extract_frontmatter_valid() {
        let content = "---\nname: test\n---\n# Body";
        let fm = SkillScanner::extract_frontmatter(content);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("name: test"));
    }

    #[test]
    fn extract_frontmatter_none() {
        let content = "# No frontmatter";
        assert!(SkillScanner::extract_frontmatter(content).is_none());
    }
}
