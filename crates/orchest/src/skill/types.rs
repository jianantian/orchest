//! Skill manifest, dependency, and capability data types.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

impl SkillManifest {
    pub fn build_tool_metadata(&self, _tool: &BundledToolDef) -> ToolMetadata {
        ToolMetadata {
            side_effect: false,
            approval: crate::tool::Approval::Never,
            source: ToolSource::Skill {
                skill_name: self.name.clone(),
            },
            ..ToolMetadata::default()
        }
    }
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
