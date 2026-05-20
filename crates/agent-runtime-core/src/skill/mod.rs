use std::path::{Path, PathBuf};

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
    pub raw_frontmatter: Value,
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

#[derive(Debug, Deserialize)]
struct RawFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    bundled_tools: Vec<RawBundledTool>,
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

        Some(SkillManifest {
            name: raw.name,
            description: raw.description,
            path: abs_dir,
            allowed_tools: raw.allowed_tools,
            bundled_tools,
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
