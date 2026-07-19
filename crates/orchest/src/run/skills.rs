//! Skill scanning and bundled tool registration.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::skill::bundled_tool::SkillBundledTool;
use crate::skill::disclosure::{LoadSkillTool, SkillSummary};
use crate::skill::executor::BareSubprocessExecutor;
use crate::skill::{CapabilityValidator, SkillScanner};
use crate::tool::builtin::ReadFileTool;
use crate::tool::registry::ToolRegistry;
use crate::tool::Tool;

use super::config::{SkillDisclosure, SkillsConfig};
use super::helpers::emit;

/// Outcome of skill registration: the skills exposed to progressive
/// disclosure (empty when nothing scanned clean, `skills.dir` is unset, or
/// disclosure is `Off`).
#[derive(Debug)]
pub(crate) struct SkillRegistration {
    pub disclosed: Vec<SkillSummary>,
}

pub(crate) async fn register_skills(
    skills_config: &SkillsConfig,
    registry: &mut ToolRegistry,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> Result<SkillRegistration, String> {
    use std::path::Path;

    let empty = SkillRegistration {
        disclosed: Vec::new(),
    };
    let Some(ref skills_dir) = skills_config.dir else {
        return Ok(empty);
    };

    let dir = Path::new(skills_dir).to_path_buf();
    let outcome = tokio::task::spawn_blocking(move || SkillScanner::scan(&dir))
        .await
        .map_err(|e| format!("skill scan join error: {e}"))?
        .map_err(|e| format!("skill scan failed: {e}"))?;

    // Surface every failed skill before the happy path: a skill that vanished
    // silently is undebuggable. The event stream is the product-facing channel;
    // tracing is the diagnostic fallback.
    for warning in &outcome.warnings {
        tracing::warn!(
            path = %warning.path.display(),
            reason = %warning.reason,
            "skill failed to load"
        );
        emit(
            tx,
            RuntimeEvent::SkillLoadWarning {
                path: warning.path.display().to_string(),
                reason: warning.reason.clone(),
            },
        )
        .await;
    }

    let manifests = outcome.manifests;
    if manifests.is_empty() {
        return Ok(empty);
    }

    let read_file_tool = Arc::new(ReadFileTool::new());
    let executor: Arc<dyn crate::skill::executor::ScriptExecutor> =
        Arc::new(BareSubprocessExecutor::new());
    let mut disclosed = Vec::new();

    for manifest in &manifests {
        // Filter by allowed_skills
        if let Some(ref allowed) = skills_config.allowed {
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

        // With disclosure off, nothing is exposed to the disclosure chain:
        // no prompt injection and no load_skill tool.
        if skills_config.disclosure != SkillDisclosure::Off {
            disclosed.push(SkillSummary {
                name: manifest.name.clone(),
                description: manifest.description.clone(),
                dir: manifest.path.clone(),
            });
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

    // Progressive disclosure: the load_skill tool resolves skill content by
    // name from the scanned manifest paths, so the model never needs a
    // handwritten path. Disabled entirely by `skill_disclosure: Off`.
    if skills_config.disclosure != SkillDisclosure::Off && !disclosed.is_empty() {
        registry
            .register(Arc::new(LoadSkillTool::new(disclosed.clone())))
            .map_err(|e| format!("failed to register load_skill tool: {e}"))?;
    }

    Ok(SkillRegistration { disclosed })
}
