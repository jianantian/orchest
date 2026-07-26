//! SKILL.md scanner: discovers and parses skill manifests from the filesystem.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::types::{
    skill_name_violations, BundledToolDef, ScanError, ScanOutcome, ScanWarning, SkillCapabilities,
    SkillDependencies, SkillManifest,
};

pub struct SkillScanner;

/// Frontmatter accepted by the scanner. Beyond the official Agent Skills
/// fields, orchest honors its own extensions (`bundled_tools`,
/// `dependencies`, `capabilities`) — the official spec does not define them,
/// so there is no hyphen/underscore variant to reconcile. The official
/// optional `license`/`compatibility`/`metadata` fields are not decoded here
/// but stay available verbatim in `SkillManifest::raw_frontmatter`.
#[derive(Debug, Deserialize)]
struct RawFrontmatter {
    name: String,
    description: String,
    // The official spec spells this field `allowed-tools` (hyphen) and types
    // it as a space-separated string; orchest historically used
    // `allowed_tools` (underscore) with a YAML list. Accept both spellings
    // and both value shapes.
    #[serde(
        default,
        alias = "allowed-tools",
        deserialize_with = "deserialize_allowed_tools"
    )]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    bundled_tools: Vec<RawBundledTool>,
    #[serde(default)]
    dependencies: SkillDependencies,
    #[serde(default)]
    capabilities: Option<RawCapabilities>,
}

/// Accepts `allowed-tools`/`allowed_tools` as either the official
/// space-separated string (`"Bash(git:*) Read"`) or a YAML list of names.
fn deserialize_allowed_tools<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrList {
        String(String),
        List(Vec<String>),
    }
    Ok(
        Option::<StringOrList>::deserialize(deserializer)?.map(|value| match value {
            StringOrList::String(s) => s.split_whitespace().map(str::to_string).collect(),
            StringOrList::List(list) => list,
        }),
    )
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
    /// Recursively scans `dir` for skills. A SKILL.md that cannot be parsed
    /// is skipped with a warning; a parsed skill that violates the Agent
    /// Skills spec (name/description rules) still loads under its declared
    /// name, with one warning per violation. Missing directories and
    /// empty scans are not errors.
    pub fn scan(dir: &Path) -> Result<ScanOutcome, ScanError> {
        let mut outcome = ScanOutcome::default();

        if !dir.exists() {
            return Ok(outcome);
        }

        Self::scan_recursive(dir, &mut outcome);
        Ok(outcome)
    }

    fn scan_recursive(dir: &Path, outcome: &mut ScanOutcome) {
        let entries = match std::fs::read_dir(dir) {
            // allow-blocking-io: called inside spawn_blocking
            Ok(e) => e,
            Err(e) => {
                outcome.warnings.push(ScanWarning {
                    path: dir.to_path_buf(),
                    reason: format!("failed to read directory: {e}"),
                });
                return;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    outcome.warnings.push(ScanWarning {
                        path: dir.to_path_buf(),
                        reason: format!("failed to read directory entry: {e}"),
                    });
                    continue;
                }
            };
            let path = entry.path();
            if path.is_dir() {
                let skill_md = Self::find_skill_md(&path);
                if let Some(md_path) = skill_md {
                    match Self::parse_skill_md(&md_path, &path) {
                        Ok((manifest, spec_warnings)) => {
                            outcome.manifests.push(manifest);
                            outcome.warnings.extend(spec_warnings);
                        }
                        Err(warning) => outcome.warnings.push(warning),
                    }
                } else {
                    Self::scan_recursive(&path, outcome);
                }
            }
        }
    }

    fn find_skill_md(dir: &Path) -> Option<PathBuf> {
        // Match exact on-disk filenames instead of `exists()`: on
        // case-insensitive filesystems `SKILL.md`.exists() also succeeds for
        // a lowercase skill.md, which would record the wrong casing.
        // allow-blocking-io: called inside spawn_blocking
        let names: Vec<std::ffi::OsString> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name())
            .collect();
        for name in ["SKILL.md", "skill.md"] {
            if names.iter().any(|n| n.as_os_str() == name) {
                return Some(dir.join(name));
            }
        }
        None
    }

    /// Parses one SKILL.md into a manifest plus any Agent Skills spec
    /// violations found (`Err` is reserved for SKILL.md files that cannot be
    /// parsed at all). Violations are reported but never block loading: the
    /// skill keeps its declared frontmatter `name` even when that name is
    /// invalid or differs from the directory name. This deterministic
    /// warn-and-continue behavior lets a non-conforming skill keep working
    /// while surfacing the problem through `ScanOutcome::warnings`.
    fn parse_skill_md(
        md_path: &Path,
        skill_dir: &Path,
    ) -> Result<(SkillManifest, Vec<ScanWarning>), ScanWarning> {
        // allow-blocking-io: called inside spawn_blocking
        let content = std::fs::read_to_string(md_path).map_err(|e| ScanWarning {
            path: md_path.to_path_buf(),
            reason: format!("failed to read file: {e}"),
        })?;

        let frontmatter = Self::extract_frontmatter(&content).ok_or_else(|| ScanWarning {
            path: md_path.to_path_buf(),
            reason: "missing frontmatter: expected opening and closing '---' delimiter lines"
                .to_string(),
        })?;
        let raw: RawFrontmatter = serde_yaml::from_str(&frontmatter).map_err(|e| ScanWarning {
            path: md_path.to_path_buf(),
            reason: format!("invalid frontmatter YAML: {e}"),
        })?;
        let raw_value: Value = serde_yaml::from_str(&frontmatter).map_err(|e| ScanWarning {
            path: md_path.to_path_buf(),
            reason: format!("invalid frontmatter YAML: {e}"),
        })?;

        // Keep the actual manifest filename (SKILL.md or skill.md) anchored
        // at the canonicalized dir, so consumers never re-derive the casing.
        let abs_dir = std::fs::canonicalize(skill_dir).unwrap_or_else(|_| skill_dir.to_path_buf()); // allow-blocking-io: called inside spawn_blocking
        let skill_md_path = match md_path.file_name() {
            Some(file_name) => abs_dir.join(file_name),
            None => md_path.to_path_buf(),
        };

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
        let spec_warnings = Self::spec_warnings(&raw.name, &raw.description, skill_dir, md_path);

        Ok((
            SkillManifest {
                name: raw.name,
                description: raw.description,
                path: abs_dir,
                skill_md_path,
                allowed_tools: raw.allowed_tools,
                bundled_tools,
                dependencies: raw.dependencies,
                capabilities,
                raw_frontmatter: raw_value,
            },
            spec_warnings,
        ))
    }

    /// Agent Skills spec checks that warn without blocking: the `name` rules
    /// (`skill_name_violations`), the spec requirement that `name` matches the
    /// skill's directory name, and the 1024-character `description` limit.
    fn spec_warnings(
        name: &str,
        description: &str,
        skill_dir: &Path,
        md_path: &Path,
    ) -> Vec<ScanWarning> {
        let mut warnings = Vec::new();
        for violation in skill_name_violations(name) {
            warnings.push(ScanWarning {
                path: md_path.to_path_buf(),
                reason: format!("invalid skill name '{name}': {violation}"),
            });
        }
        if let Some(dir_name) = skill_dir.file_name().and_then(|n| n.to_str()) {
            if dir_name != name {
                warnings.push(ScanWarning {
                    path: md_path.to_path_buf(),
                    reason: format!(
                        "skill name '{name}' does not match directory name '{dir_name}'"
                    ),
                });
            }
        }
        let description_len = description.chars().count();
        if description_len > 1024 {
            warnings.push(ScanWarning {
                path: md_path.to_path_buf(),
                reason: format!("description is {description_len} characters (max 1024)"),
            });
        }
        warnings
    }

    /// Extract the YAML frontmatter block using line-level parsing: the opening
    /// `---` must occupy its own line (a newline must follow), and only a line
    /// containing exactly `---` closes the block. A `---` inside a value (e.g.
    /// in a description) therefore does not truncate the frontmatter.
    pub(crate) fn extract_frontmatter(content: &str) -> Option<String> {
        let trimmed = content.trim_start();
        let after_open = trimmed.strip_prefix("---")?;
        let body = after_open
            .strip_prefix("\r\n")
            .or_else(|| after_open.strip_prefix('\n'))?;

        let mut offset = 0;
        for line in body.split_inclusive('\n') {
            let text = line.strip_suffix('\n').unwrap_or(line);
            let text = text.strip_suffix('\r').unwrap_or(text);
            if text == "---" {
                return Some(body[..offset].to_string());
            }
            offset += line.len();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_skill(dir: &Path) {
        let skill_dir = dir.join("test-skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: test-skill
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
        let skill_dir = dir.join("v03-skill");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: v03-skill
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

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.warnings.is_empty());
        let manifests = &outcome.manifests;
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].name, "test-skill");
        assert_eq!(manifests[0].description, "A test skill for unit testing");
        assert_eq!(manifests[0].allowed_tools, Some(vec!["read_file".into()]));
        assert_eq!(manifests[0].bundled_tools.len(), 1);
        assert_eq!(manifests[0].bundled_tools[0].name, "greet");
        assert_eq!(manifests[0].bundled_tools[0].executable, "python");
    }

    #[test]
    fn scan_records_actual_skill_md_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let lower_dir = tmp.path().join("lower-skill");
        fs::create_dir_all(&lower_dir).unwrap();
        fs::write(
            lower_dir.join("skill.md"),
            "---\nname: lower-skill\ndescription: lowercase manifest\n---\n# Lower\n",
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.manifests.len(), 1);
        let manifest = &outcome.manifests[0];
        assert_eq!(manifest.skill_md_path, manifest.path.join("skill.md"));
    }

    #[test]
    fn scan_parses_dependencies_and_capabilities() {
        let tmp = tempfile::tempdir().unwrap();
        create_v03_skill(tmp.path());

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.warnings.is_empty());
        let manifest = &outcome.manifests[0];

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
        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.manifests.is_empty());
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn scan_nonexistent_dir() {
        let outcome = SkillScanner::scan(Path::new("/nonexistent/path")).unwrap();
        assert!(outcome.manifests.is_empty());
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn scan_warns_on_invalid_yaml_and_keeps_other_skills() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill(tmp.path());

        let bad_dir = tmp.path().join("bad_skill");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(
            bad_dir.join("SKILL.md"),
            "---\nname: [unclosed\n---\nbody\n",
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert_eq!(outcome.manifests.len(), 1);
        assert_eq!(outcome.manifests[0].name, "test-skill");

        assert_eq!(outcome.warnings.len(), 1);
        let warning = &outcome.warnings[0];
        assert_eq!(warning.path, bad_dir.join("SKILL.md"));
        assert!(
            warning.reason.contains("invalid frontmatter YAML"),
            "unexpected reason: {}",
            warning.reason
        );
    }

    #[test]
    fn scan_warns_on_unreadable_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        create_test_skill(tmp.path());

        // A SKILL.md that is a directory makes read_to_string fail on every
        // platform, exercising the IO-error warning path deterministically.
        let bad_dir = tmp.path().join("unreadable_skill");
        fs::create_dir_all(bad_dir.join("SKILL.md")).unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert_eq!(outcome.manifests.len(), 1);
        assert_eq!(outcome.manifests[0].name, "test-skill");

        assert_eq!(outcome.warnings.len(), 1);
        let warning = &outcome.warnings[0];
        assert_eq!(warning.path, bad_dir.join("SKILL.md"));
        assert!(
            warning.reason.contains("failed to read file"),
            "unexpected reason: {}",
            warning.reason
        );
    }

    #[test]
    fn scan_warns_on_missing_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("plain_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# No frontmatter here\n").unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.manifests.is_empty());
        assert_eq!(outcome.warnings.len(), 1);
        assert!(
            outcome.warnings[0].reason.contains("missing frontmatter"),
            "unexpected reason: {}",
            outcome.warnings[0].reason
        );
    }

    #[test]
    fn scan_warns_on_empty_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("empty_fm_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\n---\n# Body\n").unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.manifests.is_empty());
        assert_eq!(outcome.warnings.len(), 1);
        assert!(
            outcome.warnings[0]
                .reason
                .contains("invalid frontmatter YAML"),
            "unexpected reason: {}",
            outcome.warnings[0].reason
        );
    }

    #[test]
    fn scan_parses_description_containing_triple_dash() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("dash-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: dash-skill\ndescription: alpha --- beta\n---\n# Body\n",
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.manifests.len(), 1);
        assert_eq!(outcome.manifests[0].description, "alpha --- beta");
    }

    #[test]
    fn scan_parses_crlf_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("crlf-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\r\nname: crlf-skill\r\ndescription: windows line endings\r\n---\r\n# Body\r\n",
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.manifests.len(), 1);
        assert_eq!(outcome.manifests[0].name, "crlf-skill");
        assert_eq!(outcome.manifests[0].description, "windows line endings");
    }

    #[test]
    fn scan_parses_allowed_tools_both_spellings() {
        let tmp = tempfile::tempdir().unwrap();
        // Official spec: hyphen spelling with a space-separated string value.
        let hyphen_dir = tmp.path().join("hyphen-skill");
        fs::create_dir_all(&hyphen_dir).unwrap();
        fs::write(
            hyphen_dir.join("SKILL.md"),
            "---\nname: hyphen-skill\ndescription: d\nallowed-tools: Bash(git:*) Read\n---\n# Body\n",
        )
        .unwrap();
        // Orchest spelling: underscore with a YAML list value.
        let underscore_dir = tmp.path().join("underscore-skill");
        fs::create_dir_all(&underscore_dir).unwrap();
        fs::write(
            underscore_dir.join("SKILL.md"),
            "---\nname: underscore-skill\ndescription: d\nallowed_tools:\n  - read_file\n---\n# Body\n",
        )
        .unwrap();
        // Hyphen spelling with a YAML list value also parses.
        let hyphen_list_dir = tmp.path().join("hyphen-list-skill");
        fs::create_dir_all(&hyphen_list_dir).unwrap();
        fs::write(
            hyphen_list_dir.join("SKILL.md"),
            "---\nname: hyphen-list-skill\ndescription: d\nallowed-tools:\n  - write_file\n---\n# Body\n",
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert!(outcome.warnings.is_empty());
        assert_eq!(outcome.manifests.len(), 3);
        let by_name = |name: &str| {
            outcome
                .manifests
                .iter()
                .find(|m| m.name == name)
                .unwrap_or_else(|| panic!("manifest '{name}' missing"))
        };
        assert_eq!(
            by_name("hyphen-skill").allowed_tools,
            Some(vec!["Bash(git:*)".to_string(), "Read".to_string()])
        );
        assert_eq!(
            by_name("underscore-skill").allowed_tools,
            Some(vec!["read_file".to_string()])
        );
        assert_eq!(
            by_name("hyphen-list-skill").allowed_tools,
            Some(vec!["write_file".to_string()])
        );
    }

    #[test]
    fn scan_warns_on_invalid_names_but_still_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let long_name = "a".repeat(65);
        let cases: [(&str, &str, &str); 4] = [
            ("upper-skill", "Upper-Skill", "kebab-case"),
            ("underscored-skill", "under_scored", "kebab-case"),
            ("long-skill", long_name.as_str(), "1-64 characters"),
            ("pathy-skill", "../x", "kebab-case"),
        ];
        for (dir_name, skill_name, _) in &cases {
            let skill_dir = tmp.path().join(dir_name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("---\nname: {skill_name}\ndescription: d\n---\n# Body\n"),
            )
            .unwrap();
        }

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        // Every invalid-named skill still loads under its declared name.
        assert_eq!(outcome.manifests.len(), cases.len());
        for (dir_name, skill_name, fragment) in &cases {
            assert!(
                outcome.manifests.iter().any(|m| m.name == *skill_name),
                "skill '{skill_name}' should still load"
            );
            let path = tmp.path().join(dir_name).join("SKILL.md");
            assert!(
                outcome.warnings.iter().any(|w| w.path == path
                    && w.reason
                        .contains(&format!("invalid skill name '{skill_name}'"))
                    && w.reason.contains(fragment)),
                "expected '{fragment}' warning for '{skill_name}', got: {:?}",
                outcome.warnings
            );
        }
    }

    #[test]
    fn scan_warns_on_name_directory_mismatch_but_keeps_declared_name() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("dir-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: other-skill\ndescription: d\n---\n# Body\n",
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        // Deterministic rule: the declared frontmatter name wins.
        assert_eq!(outcome.manifests.len(), 1);
        assert_eq!(outcome.manifests[0].name, "other-skill");
        assert_eq!(outcome.warnings.len(), 1);
        let warning = &outcome.warnings[0];
        assert_eq!(warning.path, skill_dir.join("SKILL.md"));
        assert!(
            warning
                .reason
                .contains("skill name 'other-skill' does not match directory name 'dir-skill'"),
            "unexpected reason: {}",
            warning.reason
        );
    }

    #[test]
    fn scan_warns_on_description_over_1024_chars() {
        let tmp = tempfile::tempdir().unwrap();
        // Boundary: exactly 1024 characters is still spec-conforming.
        let ok_dir = tmp.path().join("ok-skill");
        fs::create_dir_all(&ok_dir).unwrap();
        fs::write(
            ok_dir.join("SKILL.md"),
            format!(
                "---\nname: ok-skill\ndescription: {}\n---\n# Body\n",
                "d".repeat(1024)
            ),
        )
        .unwrap();
        let long_dir = tmp.path().join("long-desc-skill");
        fs::create_dir_all(&long_dir).unwrap();
        fs::write(
            long_dir.join("SKILL.md"),
            format!(
                "---\nname: long-desc-skill\ndescription: {}\n---\n# Body\n",
                "d".repeat(1025)
            ),
        )
        .unwrap();

        let outcome = SkillScanner::scan(tmp.path()).unwrap();
        assert_eq!(outcome.manifests.len(), 2);
        assert_eq!(outcome.warnings.len(), 1);
        let warning = &outcome.warnings[0];
        assert_eq!(warning.path, long_dir.join("SKILL.md"));
        assert!(
            warning
                .reason
                .contains("description is 1025 characters (max 1024)"),
            "unexpected reason: {}",
            warning.reason
        );
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

    #[test]
    fn extract_frontmatter_keeps_inline_triple_dash() {
        let content = "---\ndescription: alpha --- beta\n---\n# Body";
        let fm = SkillScanner::extract_frontmatter(content).unwrap();
        assert!(fm.contains("alpha --- beta"));
    }

    #[test]
    fn extract_frontmatter_crlf() {
        let content = "---\r\nname: test\r\n---\r\n# Body";
        let fm = SkillScanner::extract_frontmatter(content).unwrap();
        assert!(fm.contains("name: test"));
    }

    #[test]
    fn extract_frontmatter_empty_block() {
        let content = "---\n---\n# Body";
        let fm = SkillScanner::extract_frontmatter(content).unwrap();
        assert!(fm.is_empty());
    }

    #[test]
    fn extract_frontmatter_opening_delimiter_needs_own_line() {
        // `---` followed by content on the same line is not an opening delimiter.
        assert!(SkillScanner::extract_frontmatter("---name: test\n---\n").is_none());
        // Opening `---` without a trailing newline is not a delimiter either.
        assert!(SkillScanner::extract_frontmatter("---").is_none());
    }

    #[test]
    fn extract_frontmatter_requires_closing_line() {
        // No standalone `---` line -> no frontmatter.
        assert!(SkillScanner::extract_frontmatter("---\nname: test\n").is_none());
        // An indented `---` line is not a closing delimiter.
        assert!(SkillScanner::extract_frontmatter("---\nname: test\n  ---\n").is_none());
    }
}
