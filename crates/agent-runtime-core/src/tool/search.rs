use std::collections::HashSet;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

pub struct SearchToolsTool {
    tools: Vec<super::ToolDef>,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
}

impl SearchToolsTool {
    pub fn new(tools: Vec<super::ToolDef>) -> Self {
        Self {
            tools,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "top_k": { "type": "integer", "default": 5 }
                },
                "required": ["query"]
            }),
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: ToolSource::Builtin,
            },
        }
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<super::ToolDef> {
        let query_trigrams = trigrams(query);
        let mut scored: Vec<(f32, usize, super::ToolDef)> = self
            .tools
            .iter()
            .cloned()
            .enumerate()
            .map(|(idx, tool)| {
                let haystack = format!("{} {}", tool.name, tool.description);
                (jaccard(&query_trigrams, &trigrams(&haystack)), idx, tool)
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        scored
            .into_iter()
            .take(top_k)
            .map(|(_, _, tool)| tool)
            .collect()
    }
}

#[async_trait]
impl Tool for SearchToolsTool {
    fn name(&self) -> &str {
        "search_tools"
    }

    fn description(&self) -> &str {
        "Search available tools by name and description"
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
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError {
                message: "missing required parameter 'query'".into(),
                code: Some("MISSING_PARAM".into()),
            })?;
        let top_k = input.get("top_k").and_then(Value::as_u64).unwrap_or(5) as usize;
        let results: Vec<Value> = self
            .search(query, top_k)
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema
                })
            })
            .collect();
        Ok(ToolOutput::Immediate(Value::Array(results)))
    }
}

fn trigrams(value: &str) -> HashSet<String> {
    let normalized = value.to_lowercase();
    let chars: Vec<char> = normalized.chars().collect();
    if chars.len() < 3 {
        return std::iter::once(normalized).collect();
    }
    chars
        .windows(3)
        .map(|window| window.iter().collect())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_returns_best_matching_tool_schema() {
        let tool = SearchToolsTool::new(vec![
            super::super::ToolDef {
                name: "read_file".into(),
                description: "Read a file from disk".into(),
                input_schema: json!({"type": "object"}),
            },
            super::super::ToolDef {
                name: "send_email".into(),
                description: "Send a message".into(),
                input_schema: json!({"type": "object"}),
            },
        ]);
        let ctx = ToolContext {
            run_id: crate::run::RunId::new(),
            run_depth: 0,
            tool_call_id: "search".into(),
            on_update: None,
            event_tx: None,
            webhook_base_url: None,
        };
        let output = tool
            .execute(json!({"query": "read disk file", "top_k": 1}), &ctx)
            .await
            .expect("search should succeed");
        let ToolOutput::Immediate(Value::Array(results)) = output else {
            panic!("expected array output");
        };
        assert_eq!(results[0]["name"], "read_file");
        assert!(results[0].get("input_schema").is_some());
    }
}
