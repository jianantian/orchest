//! Conversation convergence tool: validates gathered recipient info.
//!
//! The chat agent calls this when it believes it has enough information about
//! the recipient and the memory/scene. The tool returns which fields are
//! complete and which are still missing so the agent can ask naturally.

use std::sync::Arc;

use orchest::tool::in_process::InProcessTool;
use orchest::tool::{Tool, ToolMetadata, ToolOutput};

use serde_json::{json, Value};

/// Create the `collect_info` tool for the chat agent's ToolRegistry.
pub fn create_tool() -> Arc<dyn Tool> {
    let input_schema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "The recipient's name or nickname"
            },
            "scene": {
                "type": "string",
                "description": "A concrete, visual scene or memory to anchor the song"
            },
            "emotion_direction": {
                "type": "string",
                "description": "What the sender most wants to convey (e.g. gratitude, nostalgia, hope)"
            }
        },
        "required": ["name", "scene", "emotion_direction"]
    })).expect("collect_info input schema");

    let output_schema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            "complete": {"type": "boolean"},
            "name": {"type": "boolean"},
            "scene": {"type": "boolean"},
            "emotion": {"type": "boolean"},
            "missing_fields": {
                "type": "array",
                "items": {"type": "string"}
            }
        },
        "required": ["complete", "name", "scene", "emotion", "missing_fields"]
    }))
    .expect("collect_info output schema");

    let metadata = ToolMetadata::default();

    Arc::new(InProcessTool::new(
        "collect_info".to_string(),
        "Call this when you have gathered enough information about the recipient and the memory/scene. This validates the information and tells you what's still missing.".to_string(),
        input_schema,
        Some(output_schema),
        metadata,
        Arc::new(|input: Value, _ctx| {
            Box::pin(async move {
                let name = input
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                let scene = input
                    .get("scene")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                let emotion = input
                    .get("emotion_direction")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);

                let mut missing_fields: Vec<String> = Vec::new();
                if !name {
                    missing_fields.push("name".to_string());
                }
                if !scene {
                    missing_fields.push("scene".to_string());
                }
                if !emotion {
                    missing_fields.push("emotion_direction".to_string());
                }

                let complete = missing_fields.is_empty();

                let output = json!({
                    "complete": complete,
                    "name": name,
                    "scene": scene,
                    "emotion": emotion,
                    "missing_fields": missing_fields,
                });

                Ok(ToolOutput::Immediate(output))
            })
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test the core validation logic (extracted from the InProcessTool callback)
    /// to avoid needing private orchest types for ToolContext construction.
    fn validate(input: &Value) -> Value {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let scene = input
            .get("scene")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let emotion = input
            .get("emotion_direction")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        let mut missing_fields: Vec<String> = Vec::new();
        if !name {
            missing_fields.push("name".to_string());
        }
        if !scene {
            missing_fields.push("scene".to_string());
        }
        if !emotion {
            missing_fields.push("emotion_direction".to_string());
        }

        json!({
            "complete": missing_fields.is_empty(),
            "name": name,
            "scene": scene,
            "emotion": emotion,
            "missing_fields": missing_fields,
        })
    }

    #[test]
    fn collect_info_all_complete() {
        let input = json!({
            "name": "Alice",
            "scene": "walking through the park at sunset",
            "emotion_direction": "gratitude"
        });
        let result = validate(&input);
        assert!(result["complete"].as_bool().unwrap());
        assert!(result["name"].as_bool().unwrap());
        assert!(result["scene"].as_bool().unwrap());
        assert!(result["emotion"].as_bool().unwrap());
        assert!(result["missing_fields"].as_array().unwrap().is_empty());
    }

    #[test]
    fn collect_info_missing_fields() {
        let input = json!({
            "name": "Bob",
            "scene": "",
            "emotion_direction": "nostalgia"
        });
        let result = validate(&input);
        assert!(!result["complete"].as_bool().unwrap());
        assert_eq!(result["missing_fields"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["missing_fields"].as_array().unwrap()[0]
                .as_str()
                .unwrap(),
            "scene"
        );
    }

    /// Verify the tool is constructible and has the correct name.
    #[test]
    fn tool_has_correct_name() {
        let tool = create_tool();
        assert_eq!(tool.name(), "collect_info");
    }
}
