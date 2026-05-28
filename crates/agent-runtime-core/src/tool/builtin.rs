//! Built-in tools: read_file, write_file, and similar filesystem utilities.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::events::RuntimeEvent;
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

#[derive(Debug)]
pub struct ReadFileTool {
    metadata: ToolMetadata,
    input_schema: JsonSchema,
    skill_paths: Arc<RwLock<Vec<SkillPathEntry>>>,
}

#[derive(Debug)]
pub struct WriteFileTool {
    metadata: ToolMetadata,
    input_schema: JsonSchema,
}

#[derive(Debug, Clone)]
pub struct SkillPathEntry {
    pub skill_name: String,
    pub skill_md_path: PathBuf,
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadFileTool {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::Builtin,
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to read"
                    }
                },
                "required": ["path"]
            }),
            skill_paths: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn skill_paths(&self) -> Arc<RwLock<Vec<SkillPathEntry>>> {
        Arc::clone(&self.skill_paths)
    }

    pub async fn register_skill(&self, skill_name: String, skill_md_path: PathBuf) {
        let mut paths = self.skill_paths.write().await;
        paths.push(SkillPathEntry {
            skill_name,
            skill_md_path,
        });
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteFileTool {
    pub fn new() -> Self {
        Self::new_with_approval(true)
    }

    pub fn new_with_approval(requires_approval: bool) -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: true,
                requires_approval,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::Builtin,
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "UTF-8 text content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
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
        let path_str = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError {
                message: "missing required parameter 'path'".into(),
                code: Some("MISSING_PARAM".into()),
            })?;

        let path = PathBuf::from(path_str);
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError {
                message: format!("failed to read '{}': {}", path_str, e),
                code: Some("READ_ERROR".into()),
            })?;

        let canonical = path.canonicalize().ok();
        if let Some(ref canonical_path) = canonical {
            let skill_paths = self.skill_paths.read().await;
            for entry in skill_paths.iter() {
                if let Ok(skill_canonical) = entry.skill_md_path.canonicalize() {
                    if canonical_path == &skill_canonical {
                        let tokens = (content.len() as u32) / 4;
                        if let Some(ref tx) = ctx.event_tx {
                            let _ = tx
                                .send(RuntimeEvent::SkillContentRead {
                                    skill_name: entry.skill_name.clone(),
                                    file: path_str.to_string(),
                                    tokens,
                                })
                                .await;
                        }
                        break;
                    }
                }
            }
        }

        Ok(ToolOutput::Immediate(Value::String(content)))
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write UTF-8 text content to a file, creating parent directories if needed"
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

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let path_str = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError {
                message: "missing required parameter 'path'".into(),
                code: Some("MISSING_PARAM".into()),
            })?;
        let content = input
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError {
                message: "missing required parameter 'content'".into(),
                code: Some("MISSING_PARAM".into()),
            })?;

        let path = PathBuf::from(path_str);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| ToolError {
                        message: format!(
                            "failed to create parent directories for '{path_str}': {e}"
                        ),
                        code: Some("WRITE_ERROR".into()),
                    })?;
            }
        }
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| ToolError {
                message: format!("failed to write '{path_str}': {e}"),
                code: Some("WRITE_ERROR".into()),
            })?;

        Ok(ToolOutput::Immediate(json!({
            "path": path_str,
            "bytes_written": content.len()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn read_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let tool = ReadFileTool::new();
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
            webhook_base_url: None,
        };

        let result = tool
            .execute(json!({"path": file_path.to_str().unwrap()}), &ctx)
            .await;
        assert!(result.is_ok());
        match result.unwrap() {
            ToolOutput::Immediate(Value::String(content)) => {
                assert_eq!(content, "hello world");
            }
            other => panic!("expected Immediate(String), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn read_nonexistent_file() {
        let tool = ReadFileTool::new();
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
            webhook_base_url: None,
        };

        let result = tool
            .execute(json!({"path": "/nonexistent/path/file.txt"}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("failed to read"));
    }

    #[tokio::test]
    async fn missing_path_param() {
        let tool = ReadFileTool::new();
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
            webhook_base_url: None,
        };

        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("missing required"));
    }

    #[tokio::test]
    async fn emits_skill_content_read_event() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, "---\nname: test\n---\n# Skill content here").unwrap();

        let tool = ReadFileTool::new();
        tool.register_skill("my_skill".into(), skill_md.clone())
            .await;

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(16);
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: Some(event_tx),
            webhook_base_url: None,
        };

        let result = tool
            .execute(json!({"path": skill_md.to_str().unwrap()}), &ctx)
            .await;
        assert!(result.is_ok());

        let event = event_rx.try_recv();
        assert!(event.is_ok());
        match event.unwrap() {
            RuntimeEvent::SkillContentRead {
                skill_name,
                file,
                tokens,
            } => {
                assert_eq!(skill_name, "my_skill");
                assert_eq!(file, skill_md.to_str().unwrap());
                assert!(tokens > 0);
            }
            other => panic!("expected SkillContentRead, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn no_event_for_non_skill_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("random.txt");
        fs::write(&file_path, "some content").unwrap();

        let tool = ReadFileTool::new();

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(16);
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: Some(event_tx),
            webhook_base_url: None,
        };

        let result = tool
            .execute(json!({"path": file_path.to_str().unwrap()}), &ctx)
            .await;
        assert!(result.is_ok());

        let event = event_rx.try_recv();
        assert!(event.is_err());
    }

    #[tokio::test]
    async fn write_file_creates_parent_dirs_and_writes_content() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("reports").join("final.md");

        let tool = WriteFileTool::new();
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
            webhook_base_url: None,
        };

        let result = tool
            .execute(
                json!({
                    "path": file_path.to_str().unwrap(),
                    "content": "# Final report\n\nEvidence summary."
                }),
                &ctx,
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "# Final report\n\nEvidence summary."
        );
        match result.unwrap() {
            ToolOutput::Immediate(value) => {
                assert_eq!(value["path"], file_path.to_str().unwrap());
                assert_eq!(value["bytes_written"], 33);
            }
            other => panic!("expected Immediate(Object), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn write_file_rejects_missing_content() {
        let tool = WriteFileTool::new();
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "tc_1".into(),
            on_update: None,
            event_tx: None,
            webhook_base_url: None,
        };

        let result = tool.execute(json!({"path": "report.md"}), &ctx).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code.as_deref(), Some("MISSING_PARAM"));
        assert!(error.message.contains("content"));
    }
}
