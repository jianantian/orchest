//! Text tools: search, read, write-report. Multimedia tools (ASR/vision/TTS)
//! are out of scope for this issue — they land in issue 005 in `media.rs`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use orchest::tool::{Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput};
use serde_json::{json, Value};

/// Searches the discovered text corpus for a query, returning ranked
/// `{path, snippet, score}` hits. Never requires approval — read-only.
pub struct SearchFixturesTool {
    entries: Vec<(PathBuf, String)>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl SearchFixturesTool {
    pub fn new(entries: Vec<(PathBuf, String)>) -> Self {
        Self {
            entries,
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "keywords to search for"},
                    "top_k": {"type": "integer", "description": "max results to return"}
                },
                "required": ["query"]
            }),
        }
    }
}

#[async_trait]
impl Tool for SearchFixturesTool {
    fn name(&self) -> &str {
        "search_fixtures"
    }

    fn description(&self) -> &str {
        crate::harness::SEARCH_FIXTURES_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'query'"))?;
        let top_k = input
            .get("top_k")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .max(1) as usize;

        let words: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect();

        let mut hits: Vec<(usize, &Path, String)> = self
            .entries
            .iter()
            .filter_map(|(path, content)| {
                let lower = content.to_lowercase();
                let score = words.iter().filter(|w| lower.contains(w.as_str())).count();
                if score == 0 {
                    return None;
                }
                let snippet = content
                    .lines()
                    .find(|line| {
                        let lower_line = line.to_lowercase();
                        words.iter().any(|w| lower_line.contains(w.as_str()))
                    })
                    .map(|line| line.trim().to_string())
                    .unwrap_or_else(|| content.chars().take(160).collect());
                Some((score, path.as_path(), snippet))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        hits.truncate(top_k);

        let results: Vec<Value> = hits
            .into_iter()
            .map(|(score, path, snippet)| {
                json!({"path": path.display().to_string(), "snippet": snippet, "score": score})
            })
            .collect();

        Ok(ToolOutput::Immediate(json!(results)))
    }
}

/// Reads the full contents of one fixture discovered via `search_fixtures`,
/// returning content alongside its source path. Restricted to the materials
/// corpus known at construction time. Never requires approval — read-only.
pub struct ReadFixtureTool {
    allowed: Vec<PathBuf>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl ReadFixtureTool {
    pub fn new(allowed: Vec<PathBuf>) -> Self {
        Self {
            allowed,
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Never,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "a path returned by search_fixtures"}
                },
                "required": ["path"]
            }),
        }
    }
}

#[async_trait]
impl Tool for ReadFixtureTool {
    fn name(&self) -> &str {
        "read_fixture"
    }

    fn description(&self) -> &str {
        crate::harness::READ_FIXTURE_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let path_str = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'path'"))?;
        let path = PathBuf::from(path_str);
        if !self.allowed.iter().any(|p| p == &path) {
            return Err(ToolError::invalid_input(format!(
                "'{path_str}' is not part of the materials corpus"
            ))
            .with_code("PATH_NOT_ALLOWED"));
        }
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ToolError::fatal(format!("reading {path_str}: {e}")).with_code("READ_FAILED")
        })?;
        Ok(ToolOutput::Immediate(
            json!({"path": path_str, "content": content}),
        ))
    }
}

/// Writes the final Markdown brief to a fixed output path decided by the
/// CLI (not model-supplied), so the model can only ever produce content, not
/// choose where on disk it lands. Always requires approval — this is the
/// demo's one side-effecting write.
pub struct WriteReportTool {
    output_path: PathBuf,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl WriteReportTool {
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            output_path,
            metadata: ToolMetadata {
                side_effect: true,
                approval: Approval::Always,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {"type": "string", "description": "final Markdown brief content"}
                },
                "required": ["content"]
            }),
        }
    }
}

#[async_trait]
impl Tool for WriteReportTool {
    fn name(&self) -> &str {
        "write_report"
    }

    fn description(&self) -> &str {
        crate::harness::WRITE_REPORT_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let content = input
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'content'"))?;

        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::fatal(format!("creating {}: {e}", parent.display()))
                    .with_code("WRITE_FAILED")
            })?;
        }
        std::fs::write(&self.output_path, content).map_err(|e| {
            ToolError::fatal(format!("writing {}: {e}", self.output_path.display()))
                .with_code("WRITE_FAILED")
        })?;

        Ok(ToolOutput::Immediate(json!({
            "path": self.output_path.display().to_string(),
            "bytes_written": content.len(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest::tool::{ErrorKind, RetryHint};

    fn test_ctx() -> ToolContext {
        ToolContext::oneshot()
    }

    #[tokio::test]
    async fn search_missing_query_is_invalid_input() {
        let tool = SearchFixturesTool::new(vec![]);
        let err = tool
            .execute(json!({}), &test_ctx())
            .await
            .expect_err("missing query should error");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(err.retry, RetryHint::Safe);
    }

    #[tokio::test]
    async fn read_disallowed_path_is_rejected_with_structured_error() {
        let tool = ReadFixtureTool::new(vec![PathBuf::from("allowed.md")]);
        let err = tool
            .execute(json!({"path": "not-allowed.md"}), &test_ctx())
            .await
            .expect_err("disallowed path should error");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(err.code.as_deref(), Some("PATH_NOT_ALLOWED"));
    }

    #[tokio::test]
    async fn write_report_missing_content_is_invalid_input() {
        let tool = WriteReportTool::new(PathBuf::from("/tmp/does-not-matter.md"));
        let err = tool
            .execute(json!({}), &test_ctx())
            .await
            .expect_err("missing content should error");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn search_ranks_matching_entries_and_respects_top_k() {
        let entries = vec![
            (PathBuf::from("a.md"), "no match here".to_string()),
            (
                PathBuf::from("b.md"),
                "retention retention numbers".to_string(),
            ),
            (PathBuf::from("c.md"), "one retention mention".to_string()),
        ];
        let tool = SearchFixturesTool::new(entries);
        let output = tool
            .execute(json!({"query": "retention", "top_k": 1}), &test_ctx())
            .await
            .expect("search should succeed");
        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        let results = value.as_array().expect("array output");
        assert_eq!(results.len(), 1, "top_k=1 should return exactly one hit");
        assert_eq!(
            results[0]["path"], "b.md",
            "highest-scoring entry should rank first"
        );
    }

    #[tokio::test]
    async fn read_allowed_path_returns_content_with_source_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("note.md");
        std::fs::write(&path, "hello world").expect("write fixture");

        let tool = ReadFixtureTool::new(vec![path.clone()]);
        let output = tool
            .execute(json!({"path": path.to_str().unwrap()}), &test_ctx())
            .await
            .expect("read should succeed");
        let ToolOutput::Immediate(value) = output else {
            panic!("expected immediate output");
        };
        assert_eq!(value["content"], "hello world");
        assert_eq!(value["path"], path.to_str().unwrap());
    }

    #[tokio::test]
    async fn write_report_approved_writes_exactly_one_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output_path = dir.path().join("brief.md");
        let tool = WriteReportTool::new(output_path.clone());

        tool.execute(json!({"content": "# Brief"}), &test_ctx())
            .await
            .expect("write should succeed");

        assert_eq!(std::fs::read_to_string(&output_path).unwrap(), "# Brief");
        assert!(
            tool.metadata().side_effect,
            "write tool must mark side_effect"
        );
        assert_eq!(tool.metadata().approval, Approval::Always);
    }
}
