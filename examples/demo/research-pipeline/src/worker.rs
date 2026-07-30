//! Deterministic delegated-worker tools and configuration.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use orchest::{
    model::ModelAdapter,
    run::{AgentConfig, ConfigError},
    tool::{
        agent_as_tool::ContextMode,
        registry::{RegistryError, ToolRegistry},
        Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
    },
};
use serde_json::{json, Value};
use thiserror::Error;

use crate::fault::{ControlledFaultAbortHook, FaultTriggerTool};

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("worker filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid worker configuration: {0}")]
    Config(#[from] ConfigError),
    #[error("worker tool registration failed: {0}")]
    Registry(#[from] RegistryError),
}

/// Reusable delegated-worker component. The supervisor decides how to expose
/// it (normally through `AgentAsTool`); this component starts no independent
/// CLI or run.
pub struct Worker {
    config: AgentConfig,
    registry: ToolRegistry,
}

impl Worker {
    pub fn from_paths(
        materials_directory: &Path,
        draft_path: PathBuf,
    ) -> Result<Self, WorkerError> {
        let search = SearchCorpusTool::from_directory(materials_directory)?;
        let corpus_paths = search.corpus_paths();
        let config = AgentConfig::builder("research-pipeline/worker")
            .system_prompt(
                "You are the Research Pipeline worker. Search the supplied corpus, read \
                 relevant files, and write an evidence-backed draft. Call fault_trigger \
                 only when the delegated request explicitly asks for the controlled fault.",
            )
            .max_steps(8)
            .repeated_failure_threshold(1)
            .build()?
            .with_hook(Arc::new(ControlledFaultAbortHook));

        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(search))?;
        registry.register(Arc::new(ReadFileTool::new(corpus_paths)))?;
        registry.register(Arc::new(WriteDraftTool::new(draft_path)))?;
        registry.register(Arc::new(FaultTriggerTool::new()))?;

        Ok(Self { config, registry })
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    pub fn as_tool(
        &self,
        name: &str,
        description: &str,
        model: Arc<dyn ModelAdapter>,
        context_mode: ContextMode,
    ) -> Result<Arc<dyn Tool>, ConfigError> {
        self.config
            .clone()
            .as_tool(name, description)
            .model(model)
            .registry(self.registry.clone())
            .context_mode(context_mode)
            .build()
    }
}

/// Searches a fixed, construction-time snapshot of Markdown research files.
pub struct SearchCorpusTool {
    entries: Vec<(PathBuf, String)>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl SearchCorpusTool {
    pub fn from_directory(directory: &Path) -> Result<Self, WorkerError> {
        let mut paths = std::fs::read_dir(directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .collect::<Vec<_>>();
        paths.sort();

        let entries = paths
            .into_iter()
            .map(|path| std::fs::read_to_string(&path).map(|content| (path, content)))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            entries,
            metadata: read_only_metadata(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "keywords to search for"
                    },
                    "top_k": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "maximum number of results"
                    }
                },
                "required": ["query"]
            }),
        })
    }

    pub fn corpus_paths(&self) -> Vec<PathBuf> {
        self.entries.iter().map(|(path, _)| path.clone()).collect()
    }
}

#[async_trait]
impl Tool for SearchCorpusTool {
    fn name(&self) -> &str {
        "search_corpus"
    }

    fn description(&self) -> &str {
        "Search the configured research corpus and return ranked paths and snippets."
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
        let words = query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        if words.is_empty() {
            return Err(ToolError::invalid_input("'query' must not be empty"));
        }

        let top_k = input
            .get("top_k")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .max(1) as usize;
        let mut hits = self
            .entries
            .iter()
            .filter_map(|(path, content)| {
                let lowercase = content.to_lowercase();
                let score = words
                    .iter()
                    .filter(|word| lowercase.contains(word.as_str()))
                    .count();
                if score == 0 {
                    return None;
                }
                let snippet = content
                    .lines()
                    .find(|line| {
                        let lowercase = line.to_lowercase();
                        words.iter().any(|word| lowercase.contains(word.as_str()))
                    })
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_string();
                Some((score, path, snippet))
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(right.1)));
        hits.truncate(top_k);

        Ok(ToolOutput::Immediate(json!(hits
            .into_iter()
            .map(|(score, path, snippet)| {
                json!({
                    "path": path.display().to_string(),
                    "snippet": snippet,
                    "score": score
                })
            })
            .collect::<Vec<_>>())))
    }
}

/// Reads only files captured from the configured corpus.
pub struct ReadFileTool {
    allowed_paths: Vec<PathBuf>,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl ReadFileTool {
    pub fn new(mut allowed_paths: Vec<PathBuf>) -> Self {
        allowed_paths.sort();
        allowed_paths.dedup();
        Self {
            allowed_paths,
            metadata: read_only_metadata(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "a path returned by search_corpus"
                    }
                },
                "required": ["path"]
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
        "Read a text file returned by search_corpus."
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
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("missing required parameter 'path'"))?;
        let path_buf = PathBuf::from(path);
        if !self
            .allowed_paths
            .iter()
            .any(|allowed| allowed == &path_buf)
        {
            return Err(ToolError::invalid_input(format!(
                "'{path}' is not part of the research corpus"
            ))
            .with_code("PATH_NOT_ALLOWED"));
        }

        let content = tokio::fs::read_to_string(&path_buf)
            .await
            .map_err(|error| {
                ToolError::fatal(format!("reading {path}: {error}")).with_code("READ_FAILED")
            })?;
        Ok(ToolOutput::Immediate(
            json!({"path": path, "content": content}),
        ))
    }
}

/// Writes a Markdown draft to the application-owned output path.
pub struct WriteDraftTool {
    output_path: PathBuf,
    metadata: ToolMetadata,
    input_schema: Value,
}

impl WriteDraftTool {
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            output_path,
            metadata: ToolMetadata {
                side_effect: true,
                approval: Approval::Always,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Markdown research draft"
                    }
                },
                "required": ["content"]
            }),
        }
    }
}

#[async_trait]
impl Tool for WriteDraftTool {
    fn name(&self) -> &str {
        "write_draft"
    }

    fn description(&self) -> &str {
        "Write the research draft to the configured application-owned path."
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
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                ToolError::fatal(format!("creating {}: {error}", parent.display()))
                    .with_code("WRITE_FAILED")
            })?;
        }
        tokio::fs::write(&self.output_path, content)
            .await
            .map_err(|error| {
                ToolError::fatal(format!("writing {}: {error}", self.output_path.display()))
                    .with_code("WRITE_FAILED")
            })?;
        Ok(ToolOutput::Immediate(json!({
            "path": self.output_path.display().to_string(),
            "bytes_written": content.len()
        })))
    }
}

fn read_only_metadata() -> ToolMetadata {
    ToolMetadata {
        side_effect: false,
        approval: Approval::Never,
        source: ToolSource::InProcess,
        ..ToolMetadata::default()
    }
}
