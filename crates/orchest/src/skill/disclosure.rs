//! Progressive skill disclosure: system-prompt metadata block and the
//! built-in `load_skill` tool (levels 1-3 of the Anthropic Agent Skills
//! disclosure model). Level 1 keeps every skill's name/description resident in
//! the system prompt; level 2 loads a skill's SKILL.md body plus bundled file
//! listing on demand; level 3 loads individual bundled files.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::events::RuntimeEvent;
use crate::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};

/// One scanned skill exposed to the disclosure chain. `dir` is the skill
/// directory as resolved by the scanner (already canonicalized), so
/// `load_skill` never depends on the process working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillSummary {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    /// The manifest file the scanner actually parsed (`SKILL.md` or
    /// `skill.md`) — used directly instead of re-deriving the filename,
    /// which misbehaves on case-insensitive filesystems.
    pub skill_md_path: PathBuf,
}

/// Renders the level-1 metadata block injected into the system prompt. The
/// format is fixed so applications can snapshot/assert it.
pub(crate) fn render_available_skills_block(skills: &[SkillSummary]) -> String {
    let mut block = String::from("<available_skills>\n");
    for skill in skills {
        block.push_str("<skill>\n<name>");
        block.push_str(&skill.name);
        block.push_str("</name>\n<description>");
        block.push_str(&skill.description);
        block.push_str("</description>\n</skill>\n");
    }
    block.push_str(
        "</available_skills>\n\n\
         When the user's request matches one of the skills above, call the `load_skill` tool \
         with the skill's `name` to load its full instructions before proceeding. If those \
         instructions reference bundled files, read them with `load_skill`'s optional `path` \
         argument.",
    );
    block
}

/// Appends the disclosure block to an existing system prompt; with an empty
/// prompt the block stands alone (the runtime turns it into its own System
/// message). With no disclosed skills the prompt is returned unchanged.
pub(crate) fn with_disclosure_block(system_prompt: &str, skills: &[SkillSummary]) -> String {
    if skills.is_empty() {
        return system_prompt.to_string();
    }
    let block = render_available_skills_block(skills);
    if system_prompt.is_empty() {
        block
    } else {
        format!("{system_prompt}\n\n{block}")
    }
}

/// Built-in `load_skill` tool: resolves skill content by name from the
/// scanner's manifest paths — the model never needs to know where skills live
/// on disk.
pub(crate) struct LoadSkillTool {
    metadata: ToolMetadata,
    input_schema: JsonSchema,
    skills: Vec<SkillSummary>,
}

impl std::fmt::Debug for LoadSkillTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadSkillTool")
            .field("skills", &self.skills)
            .finish_non_exhaustive()
    }
}

impl LoadSkillTool {
    pub(crate) fn new(skills: Vec<SkillSummary>) -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Never,
                source: ToolSource::Builtin,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name of the skill to load (see <available_skills> in the system prompt)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional path of a bundled file inside the skill, relative to the skill directory"
                    }
                },
                "required": ["name"]
            }),
            skills,
        }
    }

    fn find_skill(&self, name: &str) -> Result<&SkillSummary, ToolError> {
        self.skills
            .iter()
            .find(|skill| skill.name == name)
            .ok_or_else(|| {
                let available = self
                    .skills
                    .iter()
                    .map(|skill| skill.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                ToolError::invalid_input(format!(
                    "unknown skill '{name}' (available skills: {available})"
                ))
                .with_code("UNKNOWN_SKILL")
            })
    }

    /// Resolves a bundled file path inside `skill_dir`, rejecting anything
    /// that escapes the directory (relative `..`, absolute paths, symlinked
    /// ancestors) — same canonicalize + prefix-check pattern as
    /// `SkillBundledTool::new_with_options`.
    async fn resolve_bundled_path(skill_dir: &Path, relative: &str) -> Result<PathBuf, ToolError> {
        let dir_canonical = tokio::fs::canonicalize(skill_dir).await.map_err(|e| {
            ToolError::fatal(format!(
                "failed to resolve skill directory '{}': {}",
                skill_dir.display(),
                e
            ))
            .with_code("SKILL_DIR_ERROR")
        })?;
        let candidate = tokio::fs::canonicalize(dir_canonical.join(relative))
            .await
            .map_err(|e| {
                ToolError::fatal(format!("failed to resolve bundled file '{relative}': {e}"))
                    .with_code("READ_ERROR")
            })?;
        if !candidate.starts_with(&dir_canonical) {
            return Err(ToolError::fatal(format!(
                "bundled file path '{relative}' escapes skill directory '{}'",
                skill_dir.display()
            ))
            .with_code("PATH_TRAVERSAL"));
        }
        Ok(candidate)
    }

    /// Lists bundled files under `dir` as sorted `/`-joined relative paths,
    /// excluding SKILL.md itself. Symlinks are not followed.
    async fn list_bundled_files(dir: &Path, skill_md: &Path) -> Result<Vec<String>, ToolError> {
        let mut files = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&current).await.map_err(|e| {
                ToolError::fatal(format!(
                    "failed to list directory '{}': {}",
                    current.display(),
                    e
                ))
                .with_code("READ_ERROR")
            })?;
            while let Some(entry) = entries.next_entry().await.map_err(|e| {
                ToolError::fatal(format!(
                    "failed to read directory entry in '{}': {}",
                    current.display(),
                    e
                ))
                .with_code("READ_ERROR")
            })? {
                let path = entry.path();
                let file_type = entry.file_type().await.map_err(|e| {
                    ToolError::fatal(format!("failed to stat '{}': {}", path.display(), e))
                        .with_code("READ_ERROR")
                })?;
                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() && path != skill_md {
                    let relative = path.strip_prefix(dir).unwrap_or(path.as_path());
                    files.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        files.sort();
        Ok(files)
    }

    async fn emit_content_read(
        &self,
        ctx: &ToolContext,
        skill_name: &str,
        file: &Path,
        len: usize,
    ) {
        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(RuntimeEvent::SkillContentRead {
                    skill_name: skill_name.to_string(),
                    file: file.display().to_string(),
                    tokens: (len as u32) / 4,
                })
                .await;
        }
    }

    async fn execute_skill_md(
        &self,
        skill: &SkillSummary,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        // The scanner already resolved the actual manifest filename
        // (SKILL.md or skill.md); re-deriving it here would misfire on
        // case-insensitive filesystems.
        let skill_md = skill.skill_md_path.clone();
        let content = tokio::fs::read_to_string(&skill_md).await.map_err(|e| {
            ToolError::fatal(format!("failed to read '{}': {}", skill_md.display(), e))
                .with_code("READ_ERROR")
        })?;
        let bundled_files = Self::list_bundled_files(&skill.dir, &skill_md).await?;
        self.emit_content_read(ctx, &skill.name, &skill_md, content.len())
            .await;
        Ok(ToolOutput::Immediate(json!({
            "name": skill.name,
            "skill_md": content,
            "bundled_files": bundled_files,
        })))
    }

    async fn execute_bundled_file(
        &self,
        skill: &SkillSummary,
        relative: &str,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let path = Self::resolve_bundled_path(&skill.dir, relative).await?;
        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            ToolError::fatal(format!("failed to read '{relative}': {e}")).with_code("READ_ERROR")
        })?;
        self.emit_content_read(ctx, &skill.name, &path, content.len())
            .await;
        Ok(ToolOutput::Immediate(Value::String(content)))
    }
}

#[async_trait]
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load a skill's full SKILL.md instructions and bundled file listing by name; \
         pass the optional `path` argument to read a specific bundled file"
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let name = input.get("name").and_then(Value::as_str).ok_or_else(|| {
            ToolError::fatal("missing required parameter 'name'").with_code("MISSING_PARAM")
        })?;
        let skill = self.find_skill(name)?;
        match input.get("path").and_then(Value::as_str) {
            Some(relative) => self.execute_bundled_file(skill, relative, ctx).await,
            None => self.execute_skill_md(skill, ctx).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn summary(name: &str, description: &str, dir: &Path) -> SkillSummary {
        SkillSummary {
            name: name.to_string(),
            description: description.to_string(),
            dir: dir.to_path_buf(),
            skill_md_path: dir.join("SKILL.md"),
        }
    }

    fn test_ctx(event_tx: Option<tokio::sync::mpsc::Sender<RuntimeEvent>>) -> ToolContext {
        ToolContext {
            event_tx,
            ..ToolContext::oneshot()
        }
    }

    fn create_skill(root: &Path) -> PathBuf {
        let dir = root.join("lyrics-writer");
        fs::create_dir_all(dir.join("references")).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: lyrics-writer\ndescription: Writes song lyrics\n---\n# Lyrics Writer\n\nSee references/rhymes.md.\n",
        )
        .unwrap();
        fs::write(dir.join("references/rhymes.md"), "moon / june\n").unwrap();
        fs::write(dir.join("notes.txt"), "scratch\n").unwrap();
        dir
    }

    #[test]
    fn renders_stable_block_format() {
        let skills = vec![
            summary(
                "lyrics-writer",
                "Writes song lyrics",
                Path::new("/skills/lyrics-writer"),
            ),
            summary(
                "transcribe",
                "Transcribes audio",
                Path::new("/skills/transcribe"),
            ),
        ];
        let block = render_available_skills_block(&skills);
        let expected = "<available_skills>\n\
                        <skill>\n<name>lyrics-writer</name>\n<description>Writes song lyrics</description>\n</skill>\n\
                        <skill>\n<name>transcribe</name>\n<description>Transcribes audio</description>\n</skill>\n\
                        </available_skills>\n\n\
                        When the user's request matches one of the skills above, call the `load_skill` tool \
                        with the skill's `name` to load its full instructions before proceeding. If those \
                        instructions reference bundled files, read them with `load_skill`'s optional `path` \
                        argument.";
        assert_eq!(block, expected);
    }

    #[test]
    fn appends_block_to_existing_prompt() {
        let skills = vec![summary("a", "desc a", Path::new("/a"))];
        let prompt = with_disclosure_block("you are helpful", &skills);
        assert!(prompt.starts_with("you are helpful\n\n<available_skills>"));
        assert!(prompt.contains("<name>a</name>"));
    }

    #[test]
    fn empty_prompt_gets_standalone_block() {
        let skills = vec![summary("a", "desc a", Path::new("/a"))];
        let prompt = with_disclosure_block("", &skills);
        assert!(prompt.starts_with("<available_skills>"));
        assert!(!prompt.starts_with('\n'));
    }

    #[test]
    fn no_skills_leaves_prompt_unchanged() {
        assert_eq!(
            with_disclosure_block("you are helpful", &[]),
            "you are helpful"
        );
        assert_eq!(with_disclosure_block("", &[]), "");
    }

    #[tokio::test]
    async fn load_skill_returns_body_and_bundled_listing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create_skill(tmp.path());
        let tool = LoadSkillTool::new(vec![summary("lyrics-writer", "Writes song lyrics", &dir)]);

        let result = tool
            .execute(json!({"name": "lyrics-writer"}), &test_ctx(None))
            .await
            .unwrap();
        let ToolOutput::Immediate(value) = result else {
            panic!("expected Immediate output");
        };
        assert_eq!(value["name"], "lyrics-writer");
        assert!(value["skill_md"]
            .as_str()
            .unwrap()
            .contains("# Lyrics Writer"));
        assert_eq!(
            value["bundled_files"],
            json!(["notes.txt", "references/rhymes.md"])
        );
    }

    #[tokio::test]
    async fn load_skill_reads_bundled_file_by_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create_skill(tmp.path());
        let tool = LoadSkillTool::new(vec![summary("lyrics-writer", "Writes song lyrics", &dir)]);

        let result = tool
            .execute(
                json!({"name": "lyrics-writer", "path": "references/rhymes.md"}),
                &test_ctx(None),
            )
            .await
            .unwrap();
        match result {
            ToolOutput::Immediate(Value::String(content)) => {
                assert_eq!(content, "moon / june\n");
            }
            other => panic!("expected Immediate(String), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn load_skill_rejects_dotdot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create_skill(tmp.path());
        fs::write(tmp.path().join("secret.txt"), "top secret").unwrap();
        let tool = LoadSkillTool::new(vec![summary("lyrics-writer", "Writes song lyrics", &dir)]);

        let err = tool
            .execute(
                json!({"name": "lyrics-writer", "path": "../secret.txt"}),
                &test_ctx(None),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code.as_deref(), Some("PATH_TRAVERSAL"));
    }

    #[tokio::test]
    async fn load_skill_rejects_absolute_path_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create_skill(tmp.path());
        let outside = tmp.path().join("secret.txt");
        fs::write(&outside, "top secret").unwrap();
        let tool = LoadSkillTool::new(vec![summary("lyrics-writer", "Writes song lyrics", &dir)]);

        let err = tool
            .execute(
                json!({"name": "lyrics-writer", "path": outside.to_str().unwrap()}),
                &test_ctx(None),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code.as_deref(), Some("PATH_TRAVERSAL"));
    }

    #[tokio::test]
    async fn load_skill_unknown_name_returns_structured_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create_skill(tmp.path());
        let tool = LoadSkillTool::new(vec![summary("lyrics-writer", "Writes song lyrics", &dir)]);

        let err = tool
            .execute(json!({"name": "nope"}), &test_ctx(None))
            .await
            .unwrap_err();
        assert_eq!(err.code.as_deref(), Some("UNKNOWN_SKILL"));
        assert_eq!(err.kind, crate::tool::ErrorKind::InvalidInput);
        assert!(err.message.contains("lyrics-writer"));
    }

    #[tokio::test]
    async fn load_skill_missing_name_param() {
        let tool = LoadSkillTool::new(vec![]);
        let err = tool.execute(json!({}), &test_ctx(None)).await.unwrap_err();
        assert_eq!(err.code.as_deref(), Some("MISSING_PARAM"));
    }

    #[tokio::test]
    async fn load_skill_emits_telemetry_for_body_and_bundled_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create_skill(tmp.path());
        let tool = LoadSkillTool::new(vec![summary("lyrics-writer", "Writes song lyrics", &dir)]);

        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let ctx = ToolContext {
            event_tx: Some(tx),
            ..ToolContext::oneshot()
        };

        tool.execute(json!({"name": "lyrics-writer"}), &ctx)
            .await
            .unwrap();
        tool.execute(json!({"name": "lyrics-writer", "path": "notes.txt"}), &ctx)
            .await
            .unwrap();

        let first = rx.try_recv().unwrap();
        match first {
            RuntimeEvent::SkillContentRead {
                skill_name,
                file,
                tokens,
            } => {
                assert_eq!(skill_name, "lyrics-writer");
                assert!(file.ends_with("SKILL.md"));
                assert!(tokens > 0);
            }
            other => panic!("expected SkillContentRead, got {:?}", other),
        }
        let second = rx.try_recv().unwrap();
        match second {
            RuntimeEvent::SkillContentRead {
                skill_name,
                file,
                tokens,
            } => {
                assert_eq!(skill_name, "lyrics-writer");
                assert!(file.ends_with("notes.txt"));
                assert!(tokens > 0);
            }
            other => panic!("expected SkillContentRead, got {:?}", other),
        }
    }
}
