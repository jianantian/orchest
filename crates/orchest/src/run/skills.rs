//! Skill scanning and bundled tool registration.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::skill::bundled_tool::SkillBundledTool;
use crate::skill::disclosure::{LoadSkillTool, SkillSummary};
use crate::skill::executor::BareSubprocessExecutor;
use crate::skill::{CapabilityValidator, SkillManifest, SkillScanner};
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

    // Surface every scan problem before the happy path — skills that failed
    // to load and loaded skills with spec violations alike: a problem that
    // vanished silently is undebuggable. The event stream is the
    // product-facing channel; tracing is the diagnostic fallback.
    for warning in &outcome.warnings {
        tracing::warn!(
            path = %warning.path.display(),
            reason = %warning.reason,
            "skill scan warning"
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

    let mut manifests = outcome.manifests;
    if manifests.is_empty() {
        return Ok(empty);
    }

    // Process manifests in ascending directory-path order so duplicate-name
    // resolution (first registration wins) and the disclosure listing do not
    // depend on filesystem iteration order.
    manifests.sort_by_key(|manifest| manifest.path.clone());

    let read_file_tool = Arc::new(ReadFileTool::new());
    let executor: Arc<dyn crate::skill::executor::ScriptExecutor> =
        Arc::new(BareSubprocessExecutor::new());
    let mut disclosed = Vec::new();
    let mut registered: HashMap<String, PathBuf> = HashMap::new();

    for manifest in &manifests {
        // Filter by allowed_skills
        if let Some(ref allowed) = skills_config.allowed {
            if !allowed.contains(&manifest.name) {
                continue;
            }
        }

        // Duplicate skill names: the first registration (in directory-path
        // order) wins; later same-name skills are skipped with a warning.
        if let Some(winner) = registered.get(&manifest.name) {
            let reason = format!(
                "duplicate skill name '{}': already registered from '{}'; skipping",
                manifest.name,
                winner.display()
            );
            report_skill_failure(skills_config.strict, tx, &manifest.skill_md_path, &reason)
                .await?;
            continue;
        }

        let bundled = match prepare_bundled_tools(manifest, registry, &executor) {
            Ok(tools) => tools,
            Err(reason) => {
                report_skill_failure(skills_config.strict, tx, &manifest.skill_md_path, &reason)
                    .await?;
                continue;
            }
        };

        for tool in bundled {
            // Names were pre-validated in prepare_bundled_tools, so
            // registration cannot conflict; propagate defensively if that
            // invariant is ever broken.
            registry
                .register(Arc::new(tool))
                .map_err(|e| format!("failed to register bundled tool: {e}"))?;
        }
        registered.insert(manifest.name.clone(), manifest.path.clone());

        // Register the scanner-resolved manifest path (SKILL.md or skill.md)
        // with read_file for SkillContentRead telemetry.
        read_file_tool
            .register_skill(manifest.name.clone(), manifest.skill_md_path.clone())
            .await;

        // With disclosure off, nothing is exposed to the disclosure chain:
        // no prompt injection and no load_skill tool.
        if skills_config.disclosure != SkillDisclosure::Off {
            disclosed.push(SkillSummary {
                name: manifest.name.clone(),
                description: manifest.description.clone(),
                dir: manifest.path.clone(),
            });
        }

        // Emit SkillMissingCapabilities warning if applicable. Only skills
        // that actually registered can run, so skipped skills never warn.
        if CapabilityValidator::missing_capabilities_warning(manifest) {
            emit(
                tx,
                RuntimeEvent::SkillMissingCapabilities {
                    skill_name: manifest.name.clone(),
                },
            )
            .await;
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

/// Validates a skill's bundled tools without mutating the registry: creates
/// every tool (executable whitelist, script resolution, path-traversal
/// checks) and verifies tool-name uniqueness within the skill and against
/// already-registered tools. On success the returned tools register without
/// conflict, so a bad tool definition skips its skill atomically instead of
/// leaving it half-registered.
fn prepare_bundled_tools(
    manifest: &SkillManifest,
    registry: &ToolRegistry,
    executor: &Arc<dyn crate::skill::executor::ScriptExecutor>,
) -> Result<Vec<SkillBundledTool>, String> {
    let mut tools = Vec::with_capacity(manifest.bundled_tools.len());
    for tool_def in &manifest.bundled_tools {
        let tool = SkillBundledTool::new_with_options(
            tool_def,
            manifest.path.clone(),
            manifest.name.clone(),
            manifest.dependencies.clone(),
            manifest.capabilities.clone(),
            Arc::clone(executor),
        )
        .map_err(|e| {
            format!(
                "failed to create bundled tool '{}' for skill '{}': {}",
                tool_def.name, manifest.name, e.message
            )
        })?;
        tools.push(tool);
    }

    let mut names = HashSet::with_capacity(tools.len());
    for tool in &tools {
        if !names.insert(tool.name()) {
            return Err(format!(
                "duplicate tool name '{}' from skill '{}'",
                tool.name(),
                manifest.name
            ));
        }
        if registry.contains(tool.name()) {
            return Err(format!(
                "duplicate tool name '{}' from skill '{}': already registered",
                tool.name(),
                manifest.name
            ));
        }
    }
    Ok(tools)
}

/// Reports a single-skill registration failure through the same channel as
/// scan warnings (tracing + `SkillLoadWarning`). In strict mode the reason
/// is returned as `Err` so startup aborts with `RunFailed`; otherwise the
/// caller skips the skill and the run starts without it.
async fn report_skill_failure(
    strict: bool,
    tx: &mpsc::Sender<RuntimeEvent>,
    path: &Path,
    reason: &str,
) -> Result<(), String> {
    tracing::warn!(
        path = %path.display(),
        reason = %reason,
        "skill skipped during registration"
    );
    emit(
        tx,
        RuntimeEvent::SkillLoadWarning {
            path: path.display().to_string(),
            reason: reason.to_string(),
        },
    )
    .await;
    if strict {
        return Err(reason.to_string());
    }
    Ok(())
}
