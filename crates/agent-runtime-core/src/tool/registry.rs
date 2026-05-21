use std::collections::HashMap;
use std::sync::Arc;

use super::{Tool, ToolDef};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("tool '{0}' is already registered")]
    DuplicateName(String),
}

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
                metadata: ToolMetadata {
                    side_effect: false,
                    requires_approval: false,
                    cost_hint: None,
                    timeout: None,
                    max_output_tokens: None,
                    source: ToolSource::InProcess,
                },
            }
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
}
