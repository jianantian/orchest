// Skill scanning and bundled tool registration.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::skill::bundled_tool::SkillBundledTool;
use crate::skill::executor::BareSubprocessExecutor;
use crate::skill::{CapabilityValidator, SkillScanner};
use crate::tool::builtin::ReadFileTool;
use crate::tool::registry::ToolRegistry;
use crate::tool::Tool;

use super::helpers::emit;

pub(crate) async fn register_skills(
    skills_dir: &str,
    allowed_skills: &Option<Vec<String>>,
    registry: &mut ToolRegistry,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> Result<Option<Arc<ReadFileTool>>, String> {
    use std::path::Path;

    let dir = Path::new(skills_dir).to_path_buf();
    let manifests = tokio::task::spawn_blocking(move || SkillScanner::scan(&dir))
        .await
        .map_err(|e| format!("skill scan join error: {e}"))?
        .map_err(|e| format!("skill scan failed: {e}"))?;
    if manifests.is_empty() {
        return Ok(None);
    }

    let read_file_tool = Arc::new(ReadFileTool::new());
    let executor: Arc<dyn crate::skill::executor::ScriptExecutor> =
        Arc::new(BareSubprocessExecutor::new());

    for manifest in &manifests {
        // Filter by allowed_skills
        if let Some(ref allowed) = allowed_skills {
            if !allowed.contains(&manifest.name) {
                continue;
            }
        }

        // Emit SkillMissingCapabilities warning if applicable
        if CapabilityValidator::missing_capabilities_warning(manifest) {
            emit(
                tx,
                RuntimeEvent::SkillMissingCapabilities {
                    skill_name: manifest.name.clone(),
                },
            )
            .await;
        }

        // Register SKILL.md path with read_file for telemetry
        let skill_md_path = manifest.path.join("SKILL.md");
        if skill_md_path.exists() {
            read_file_tool
                .register_skill(manifest.name.clone(), skill_md_path)
                .await;
        }

        // Register each bundled tool
        for tool_def in &manifest.bundled_tools {
            let bundled = SkillBundledTool::new_with_options(
                tool_def,
                manifest.path.clone(),
                manifest.name.clone(),
                manifest.dependencies.clone(),
                manifest.capabilities.clone(),
                Arc::clone(&executor),
            )
            .map_err(|e| {
                format!(
                    "failed to create bundled tool '{}' for skill '{}': {}",
                    tool_def.name, manifest.name, e.message
                )
            })?;

            registry.register(Arc::new(bundled)).map_err(|e| {
                format!(
                    "duplicate tool name '{}' from skill '{}': {}",
                    tool_def.name, manifest.name, e
                )
            })?;
        }
    }

    // Register read_file tool for skill telemetry
    registry
        .register(read_file_tool.clone() as Arc<dyn Tool>)
        .map_err(|e| format!("failed to register read_file tool: {e}"))?;

    Ok(Some(read_file_tool))
}
