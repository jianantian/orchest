//! Tool registry: name-indexed collection of available tools.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Tool, ToolDef, ToolExecutionMode};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("tool '{0}' is already registered")]
    DuplicateName(String),
    #[error("tool '{tool}' links to itself as draft/commit pair")]
    SelfLink { tool: String },
    #[error("tool '{tool}' links to missing tool '{linked_tool}'")]
    MissingLinkedTool { tool: String, linked_tool: String },
    #[error("tool '{tool}' links to '{linked_tool}', but the linked tool does not point back")]
    NonReciprocalLink { tool: String, linked_tool: String },
    #[error("tool '{linked_tool}' is linked from multiple draft/commit tools")]
    AmbiguousLink { linked_tool: String },
}

/// An ordered, name-keyed collection of `Arc<dyn Tool>`. Register tools with
/// [`ToolRegistry::register`] and pass the registry to `AgentRun::start`.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    order: Vec<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<(), RegistryError> {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            return Err(RegistryError::DuplicateName(name));
        }
        if links_to(tool.metadata(), &name) {
            return Err(RegistryError::SelfLink { tool: name });
        }
        self.order.push(name.clone());
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn list(&self) -> Vec<ToolDef> {
        self.order
            .iter()
            .filter_map(|name| {
                let tool = self.tools.get(name)?;
                Some(ToolDef {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    input_schema: tool.input_schema().clone(),
                })
            })
            .collect()
    }

    pub fn filter_by_allowed(&self, allowed: &Option<Vec<String>>) -> ToolRegistry {
        match allowed {
            None => {
                let mut filtered = ToolRegistry::new();
                for name in &self.order {
                    if let Some(tool) = self.tools.get(name) {
                        filtered.order.push(name.clone());
                        filtered.tools.insert(name.clone(), Arc::clone(tool));
                    }
                }
                filtered
            }
            Some(names) => {
                let mut filtered = ToolRegistry::new();
                for name in names {
                    if let Some(tool) = self.tools.get(name) {
                        filtered.order.push(name.clone());
                        filtered.tools.insert(name.clone(), Arc::clone(tool));
                    }
                }
                filtered
            }
        }
    }

    pub fn validate_metadata_links(&self) -> Result<(), RegistryError> {
        let mut commit_to_draft: HashMap<&str, &str> = HashMap::new();
        let mut draft_to_commit: HashMap<&str, &str> = HashMap::new();

        for name in &self.order {
            let Some(tool) = self.tools.get(name) else {
                continue;
            };
            match &tool.metadata().execution_mode {
                ToolExecutionMode::Normal => {}
                ToolExecutionMode::Draft { commit_tool } => {
                    self.validate_link_target(name, commit_tool)?;
                    if commit_to_draft
                        .insert(commit_tool.as_str(), name.as_str())
                        .is_some()
                    {
                        return Err(RegistryError::AmbiguousLink {
                            linked_tool: commit_tool.clone(),
                        });
                    }
                    let target = self.tools.get(commit_tool).expect("validated above");
                    match &target.metadata().execution_mode {
                        ToolExecutionMode::Commit { draft_tool } if draft_tool == name => {}
                        _ => {
                            return Err(RegistryError::NonReciprocalLink {
                                tool: name.clone(),
                                linked_tool: commit_tool.clone(),
                            });
                        }
                    }
                }
                ToolExecutionMode::Commit { draft_tool } => {
                    self.validate_link_target(name, draft_tool)?;
                    if draft_to_commit
                        .insert(draft_tool.as_str(), name.as_str())
                        .is_some()
                    {
                        return Err(RegistryError::AmbiguousLink {
                            linked_tool: draft_tool.clone(),
                        });
                    }
                    let target = self.tools.get(draft_tool).expect("validated above");
                    match &target.metadata().execution_mode {
                        ToolExecutionMode::Draft { commit_tool } if commit_tool == name => {}
                        _ => {
                            return Err(RegistryError::NonReciprocalLink {
                                tool: name.clone(),
                                linked_tool: draft_tool.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_link_target(&self, name: &str, linked_tool: &str) -> Result<(), RegistryError> {
        if name == linked_tool {
            return Err(RegistryError::SelfLink {
                tool: name.to_string(),
            });
        }
        if !self.tools.contains_key(linked_tool) {
            return Err(RegistryError::MissingLinkedTool {
                tool: name.to_string(),
                linked_tool: linked_tool.to_string(),
            });
        }
        Ok(())
    }
}

fn links_to(metadata: &super::ToolMetadata, name: &str) -> bool {
    match &metadata.execution_mode {
        ToolExecutionMode::Normal => false,
        ToolExecutionMode::Draft { commit_tool } => commit_tool == name,
        ToolExecutionMode::Commit { draft_tool } => draft_tool == name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::*;
    use async_trait::async_trait;
    use serde_json::json;

    struct FakeTool {
        name: String,
        metadata: ToolMetadata,
    }

    impl FakeTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                metadata: ToolMetadata::default(),
            }
        }

        fn with_execution_mode(mut self, execution_mode: ToolExecutionMode) -> Self {
            self.metadata.execution_mode = execution_mode;
            self
        }
    }

    #[async_trait]
    impl Tool for FakeTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "fake tool"
        }
        fn input_schema(&self) -> &JsonSchema {
            &serde_json::Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &self.metadata
        }
        async fn execute(
            &self,
            _input: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Immediate(json!("ok")))
        }
    }

    #[test]
    fn register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("alpha"))).unwrap();
        assert!(reg.get("alpha").is_some());
        assert!(reg.get("beta").is_none());
    }

    #[test]
    fn duplicate_name_rejected() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("alpha"))).unwrap();
        let err = reg.register(Arc::new(FakeTool::new("alpha"))).unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateName(_)));
    }

    #[test]
    fn list_preserves_order() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("beta"))).unwrap();
        reg.register(Arc::new(FakeTool::new("alpha"))).unwrap();
        let defs = reg.list();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].name, "beta");
        assert_eq!(defs[1].name, "alpha");
    }

    #[test]
    fn filter_by_allowed_none_returns_all() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("a"))).unwrap();
        reg.register(Arc::new(FakeTool::new("b"))).unwrap();
        let filtered = reg.filter_by_allowed(&None);
        assert_eq!(filtered.list().len(), 2);
    }

    #[test]
    fn filter_by_allowed_some_filters() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("a"))).unwrap();
        reg.register(Arc::new(FakeTool::new("b"))).unwrap();
        reg.register(Arc::new(FakeTool::new("c"))).unwrap();
        let filtered = reg.filter_by_allowed(&Some(vec!["a".into(), "c".into()]));
        let defs = filtered.list();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].name, "a");
        assert_eq!(defs[1].name, "c");
    }

    #[test]
    fn validate_metadata_links_accepts_reciprocal_pair() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("commit").with_execution_mode(
            ToolExecutionMode::Commit {
                draft_tool: "draft".into(),
            },
        )))
        .unwrap();
        reg.register(Arc::new(FakeTool::new("draft").with_execution_mode(
            ToolExecutionMode::Draft {
                commit_tool: "commit".into(),
            },
        )))
        .unwrap();

        reg.validate_metadata_links().unwrap();
    }

    #[test]
    fn validate_metadata_links_rejects_missing_link() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("draft").with_execution_mode(
            ToolExecutionMode::Draft {
                commit_tool: "missing".into(),
            },
        )))
        .unwrap();

        let err = reg.validate_metadata_links().unwrap_err();
        assert!(matches!(err, RegistryError::MissingLinkedTool { .. }));
    }

    #[test]
    fn register_rejects_self_link() {
        let mut reg = ToolRegistry::new();
        let err = reg
            .register(Arc::new(FakeTool::new("draft").with_execution_mode(
                ToolExecutionMode::Draft {
                    commit_tool: "draft".into(),
                },
            )))
            .unwrap_err();
        assert!(matches!(err, RegistryError::SelfLink { .. }));
    }

    #[test]
    fn validate_metadata_links_rejects_non_reciprocal_link() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("draft").with_execution_mode(
            ToolExecutionMode::Draft {
                commit_tool: "commit".into(),
            },
        )))
        .unwrap();
        reg.register(Arc::new(FakeTool::new("commit"))).unwrap();

        let err = reg.validate_metadata_links().unwrap_err();
        assert!(matches!(err, RegistryError::NonReciprocalLink { .. }));
    }

    #[test]
    fn validate_metadata_links_rejects_many_to_one_links() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FakeTool::new("draft_a").with_execution_mode(
            ToolExecutionMode::Draft {
                commit_tool: "commit".into(),
            },
        )))
        .unwrap();
        reg.register(Arc::new(FakeTool::new("draft_b").with_execution_mode(
            ToolExecutionMode::Draft {
                commit_tool: "commit".into(),
            },
        )))
        .unwrap();
        reg.register(Arc::new(FakeTool::new("commit").with_execution_mode(
            ToolExecutionMode::Commit {
                draft_tool: "draft_a".into(),
            },
        )))
        .unwrap();

        let err = reg.validate_metadata_links().unwrap_err();
        assert!(matches!(
            err,
            RegistryError::NonReciprocalLink { .. } | RegistryError::AmbiguousLink { .. }
        ));
    }
}
