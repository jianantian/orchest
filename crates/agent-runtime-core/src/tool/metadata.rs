use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::Approval;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub side_effect: bool,
    pub approval: Approval,
    #[serde(default)]
    pub execution_mode: ToolExecutionMode,
    #[serde(default)]
    pub parallelism: ToolParallelism,
    pub cost_hint: Option<CostHint>,
    pub timeout: Option<Duration>,
    pub max_output_tokens: Option<u64>,
    pub source: ToolSource,
}

impl Default for ToolMetadata {
    fn default() -> Self {
        Self {
            side_effect: false,
            approval: Approval::default(),
            execution_mode: ToolExecutionMode::default(),
            parallelism: ToolParallelism::default(),
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolExecutionMode {
    #[default]
    Normal,
    Draft {
        commit_tool: String,
    },
    Commit {
        draft_tool: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolParallelism {
    #[default]
    Serial,
    ParallelSafe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSource {
    InProcess,
    McpServer { server_id: String },
    Skill { skill_name: String },
    Builtin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CostHint {
    Free,
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_metadata_defaults_to_normal_serial() {
        let metadata = ToolMetadata::default();

        assert_eq!(metadata.execution_mode, ToolExecutionMode::Normal);
        assert_eq!(metadata.parallelism, ToolParallelism::Serial);
    }

    #[test]
    fn tool_metadata_deserializes_legacy_json_with_defaults() {
        let metadata: ToolMetadata = serde_json::from_value(json!({
            "side_effect": false,
            "approval": "Never",
            "cost_hint": null,
            "timeout": null,
            "max_output_tokens": null,
            "source": "InProcess"
        }))
        .expect("legacy metadata should deserialize");

        assert_eq!(metadata.execution_mode, ToolExecutionMode::Normal);
        assert_eq!(metadata.parallelism, ToolParallelism::Serial);
    }

    #[test]
    fn tool_execution_mode_serde_is_tagged_and_stable() {
        let mode = ToolExecutionMode::Draft {
            commit_tool: "write_file".into(),
        };

        let value = serde_json::to_value(mode).expect("serialize");

        assert_eq!(
            value,
            json!({
                "kind": "draft",
                "commit_tool": "write_file"
            })
        );
    }
}
