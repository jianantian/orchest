//! SKILL.md scanner: discovers and parses skill manifests from the filesystem.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::types::{
    BundledToolDef, ScanError, SkillCapabilities, SkillDependencies, SkillManifest,
};

pub struct SkillScanner;

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
            // allow-blocking-io: called inside spawn_blocking
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
        let content = std::fs::read_to_string(md_path).ok()?; // allow-blocking-io: called inside spawn_blocking

        let frontmatter = Self::extract_frontmatter(&content)?;
        let raw: RawFrontmatter = serde_yaml::from_str(&frontmatter).ok()?;
        let raw_value: Value = serde_yaml::from_str(&frontmatter).ok()?;

        let abs_dir = std::fs::canonicalize(skill_dir).unwrap_or_else(|_| skill_dir.to_path_buf()); // allow-blocking-io: called inside spawn_blocking

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

    pub(crate) fn extract_frontmatter(content: &str) -> Option<String> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return None;
        }

        let after_first = &trimmed[3..];
        let end = after_first.find("---")?;
        Some(after_first[..end].to_string())
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
