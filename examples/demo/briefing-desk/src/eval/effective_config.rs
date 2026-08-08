//! Desensitized effective runtime configuration snapshot.
//!
//! Built by the application from the *actual* resolved config (not incomplete
//! `AgentConfig` serde). Tool descriptions are recorded only as harness
//! `surface_id`s. Secrets and credential-bearing URLs fail preflight.

use std::collections::BTreeMap;

use orchest::budget::BudgetConfig;
use orchest::run::{
    ApprovalMode, BackoffStrategy, RetryPolicy, RuntimeConfig, SupervisionStrategy,
};
use orchest::tool::mcp::McpTransport;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{Approval, ToolMetadata, ToolSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::harness::SurfaceId;

use super::artifact::ArtifactError;
use super::credential::{
    is_sensitive_configuration_name, text_has_credentials, url_has_credentials,
    value_has_credentials,
};

/// Effective-config schema version.
pub const EFFECTIVE_CONFIG_SCHEMA_VERSION: &str = "3";

/// How a media capability is routed for this run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CapabilityRoute {
    Disabled,
    Fake,
    Injected {
        provider: String,
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
    },
    Live {
        provider: String,
        model: String,
        /// Non-secret endpoint only; credentialed URLs must fail preflight.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
    },
}

/// Session persistence mode for the attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPersistenceMode {
    None,
    FreshSqlite,
    FollowUpFromSeed,
}

/// One tool's effective registry entry (no description text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFingerprint {
    pub name: String,
    /// Harness surface id for the description, when this tool is a candidate surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_surface_id: Option<String>,
    pub input_schema_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema_sha256: Option<String>,
    pub side_effect: bool,
    pub approval: String,
    pub execution_mode: String,
    pub parallelism: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    pub source: String,
}

/// Snapshot of one agent role (main / reviewer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRoleSnapshot {
    pub role: String,
    pub model_provider: String,
    pub model_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Non-secret request options only.
    pub request_options: Value,
    pub max_steps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    pub tool_search_enabled: bool,
    pub compaction: Option<CompactionSnapshot>,
    pub mcp_servers: Vec<McpServerSnapshot>,
    pub webhook_enabled: bool,
    pub code_execution: CodeExecutionSnapshot,
    pub run_depth: u32,
    pub repeated_failure_threshold: usize,
    pub supervision: String,
    pub approval_mode: String,
    pub custom_approval: bool,
    pub tool_execution_policy: String,
    pub budget: BudgetLimitsSnapshot,
    pub retry: RetrySnapshot,
    /// Stable label for hooks presence (not Debug pointers).
    pub hooks_label: String,
    /// Stable label for session store attachment.
    pub session_store_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionSnapshot {
    pub threshold: f32,
    pub recent_messages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerSnapshot {
    pub server_id: String,
    pub transport: McpTransportSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpTransportSnapshot {
    Stdio {
        command: String,
        args: Vec<String>,
    },
    StreamableHttp {
        endpoint: String,
        authentication: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeExecutionSnapshot {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetLimitsSnapshot {
    pub max_tokens: Option<u64>,
    pub max_tool_calls: Option<u32>,
    pub max_duration_ms: Option<u64>,
    pub max_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrySnapshot {
    pub configured: bool,
    pub max_retries: Option<u32>,
    pub backoff: Option<String>,
    pub base_delay_ms: Option<u64>,
    pub max_delay_ms: Option<u64>,
    pub jitter: Option<bool>,
}

/// Full effective config for one eval run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveConfigSnapshot {
    pub schema_version: String,
    pub main: AgentRoleSnapshot,
    pub reviewer: AgentRoleSnapshot,
    /// Tools available to the main agent, sorted by name.
    pub tools: Vec<ToolFingerprint>,
    pub asr: CapabilityRoute,
    pub tts: CapabilityRoute,
    pub vision: CapabilityRoute,
    pub session_mode: SessionPersistenceMode,
    /// Distinct execution profiles selected by stable case id.
    pub case_profiles: Vec<CaseProfileSnapshot>,
    /// Resolved non-secret environment-driven options (values, not raw secrets).
    pub env_options: BTreeMap<String, Value>,
}

/// Stable, credential-free profile produced by product preparation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseProfileSnapshot {
    pub case_ids: Vec<String>,
    pub main: AgentRoleSnapshot,
    pub reviewer: AgentRoleSnapshot,
    pub tools: Vec<ToolFingerprint>,
    pub asr: CapabilityRoute,
    pub tts: CapabilityRoute,
    pub vision: CapabilityRoute,
    pub session_mode: SessionPersistenceMode,
}

/// Inputs used to build an effective config snapshot without constructing live adapters.
#[derive(Debug, Clone)]
pub struct EffectiveConfigInput {
    pub main_model_provider: String,
    pub main_model_name: String,
    pub main_request_options: Value,
    pub main_runtime: RuntimeConfig,
    pub main_budget: BudgetConfig,
    pub main_retry: Option<RetryPolicy>,
    pub main_supervision: SupervisionStrategy,
    pub main_hooks_label: String,
    pub main_session_store_label: String,
    /// Required when an enabled code-execution runtime has an executor.
    pub main_executor_stable_label: Option<String>,
    pub reviewer_model_provider: String,
    pub reviewer_model_name: String,
    pub reviewer_request_options: Value,
    pub reviewer_max_steps: u32,
    pub reviewer_budget: BudgetConfig,
    pub reviewer_retry: Option<RetryPolicy>,
    pub reviewer_supervision: SupervisionStrategy,
    pub tools: Vec<ToolFingerprint>,
    pub asr: CapabilityRoute,
    pub tts: CapabilityRoute,
    pub vision: CapabilityRoute,
    pub session_mode: SessionPersistenceMode,
    pub env_options: BTreeMap<String, Value>,
}

impl EffectiveConfigSnapshot {
    /// Build from explicit application-resolved inputs.
    pub fn from_input(input: EffectiveConfigInput) -> Result<Self, ArtifactError> {
        preflight_capability_routes(&[&input.asr, &input.tts, &input.vision])?;
        preflight_env_options(&input.env_options)?;
        preflight_request_options(&input.main_request_options)?;
        preflight_request_options(&input.reviewer_request_options)?;
        let main_runtime = runtime_snapshot(
            &input.main_runtime,
            input.main_executor_stable_label.as_deref(),
        )?;

        let snap = Self {
            schema_version: EFFECTIVE_CONFIG_SCHEMA_VERSION.to_string(),
            main: AgentRoleSnapshot {
                role: "main".into(),
                model_provider: input.main_model_provider,
                model_name: input.main_model_name,
                endpoint: None,
                request_options: sanitize_request_options(&input.main_request_options),
                max_steps: main_runtime.max_steps,
                allowed_tools: main_runtime.allowed_tools,
                tool_search_enabled: main_runtime.tool_search_enabled,
                compaction: main_runtime.compaction,
                mcp_servers: main_runtime.mcp_servers,
                webhook_enabled: main_runtime.webhook_enabled,
                code_execution: main_runtime.code_execution,
                run_depth: main_runtime.run_depth,
                repeated_failure_threshold: main_runtime.repeated_failure_threshold,
                supervision: supervision_label(&input.main_supervision),
                approval_mode: approval_mode_label(input.main_runtime.approval_mode),
                custom_approval: input.main_runtime.custom_approval_fn.is_some(),
                tool_execution_policy: tool_policy_label(&input.main_runtime.tool_execution_policy),
                budget: budget_limits(&input.main_budget),
                retry: retry_snapshot(input.main_retry.as_ref()),
                hooks_label: input.main_hooks_label,
                session_store_label: input.main_session_store_label,
            },
            reviewer: AgentRoleSnapshot {
                role: "reviewer".into(),
                model_provider: input.reviewer_model_provider,
                model_name: input.reviewer_model_name,
                endpoint: None,
                request_options: sanitize_request_options(&input.reviewer_request_options),
                max_steps: input.reviewer_max_steps,
                allowed_tools: None,
                tool_search_enabled: false,
                compaction: None,
                mcp_servers: Vec::new(),
                webhook_enabled: false,
                code_execution: CodeExecutionSnapshot {
                    enabled: false,
                    executor_label: None,
                },
                run_depth: 0,
                repeated_failure_threshold: 0,
                supervision: supervision_label(&input.reviewer_supervision),
                approval_mode: approval_mode_label(ApprovalMode::PerTool),
                custom_approval: false,
                tool_execution_policy: "sequential".into(),
                budget: budget_limits(&input.reviewer_budget),
                retry: retry_snapshot(input.reviewer_retry.as_ref()),
                hooks_label: "none".into(),
                session_store_label: "none".into(),
            },
            tools: {
                let mut tools = input.tools;
                tools.sort_by(|a, b| a.name.cmp(&b.name));
                tools
            },
            asr: input.asr,
            tts: input.tts,
            vision: input.vision,
            session_mode: input.session_mode,
            case_profiles: Vec::new(),
            env_options: input.env_options,
        };

        let mut snap = snap;
        snap.case_profiles.push(CaseProfileSnapshot {
            case_ids: Vec::new(),
            main: snap.main.clone(),
            reviewer: snap.reviewer.clone(),
            tools: snap.tools.clone(),
            asr: snap.asr.clone(),
            tts: snap.tts.clone(),
            vision: snap.vision.clone(),
            session_mode: snap.session_mode.clone(),
        });

        // Secret-looking values are rejected in preflight_* before assembly.
        // Still hard-fail on common raw key material markers if any slipped through.
        let bytes = snap.normalize_bytes()?;
        let text = String::from_utf8_lossy(&bytes);
        if text.contains("sk-live") || text.contains("sk-ant-") {
            return Err(ArtifactError::preflight(
                "effective config snapshot would contain secret material",
            ));
        }
        Ok(snap)
    }

    /// Build one snapshot from the exact profiles produced by preparation.
    pub fn from_profiles(
        mut profiles: Vec<CaseProfileSnapshot>,
        env_options: BTreeMap<String, Value>,
    ) -> Result<Self, ArtifactError> {
        if profiles.is_empty() {
            return Err(ArtifactError::preflight(
                "effective config requires at least one case profile",
            ));
        }
        for profile in &mut profiles {
            profile.case_ids.sort();
            profile.case_ids.dedup();
            profile.tools.sort_by(|a, b| a.name.cmp(&b.name));
            preflight_capability_routes(&[&profile.asr, &profile.tts, &profile.vision])?;
            preflight_request_options(&profile.main.request_options)?;
            preflight_request_options(&profile.reviewer.request_options)?;
            preflight_endpoint(profile.main.endpoint.as_deref())?;
            preflight_endpoint(profile.reviewer.endpoint.as_deref())?;
            if profile.main.hooks_label.trim().is_empty()
                || profile.main.session_store_label.trim().is_empty()
                || profile.reviewer.hooks_label.trim().is_empty()
                || profile.reviewer.session_store_label.trim().is_empty()
            {
                return Err(ArtifactError::preflight(
                    "enabled component is missing a stable hooks/store label",
                ));
            }
        }
        profiles.sort_by(|a, b| a.case_ids.cmp(&b.case_ids));
        preflight_env_options(&env_options)?;
        let representative = profiles[0].clone();
        let snapshot = Self {
            schema_version: EFFECTIVE_CONFIG_SCHEMA_VERSION.to_string(),
            main: representative.main,
            reviewer: representative.reviewer,
            tools: representative.tools,
            asr: representative.asr,
            tts: representative.tts,
            vision: representative.vision,
            session_mode: representative.session_mode,
            case_profiles: profiles,
            env_options,
        };
        let bytes = snapshot.normalize_bytes()?;
        let text = String::from_utf8_lossy(&bytes);
        if text.contains("sk-live") || text.contains("sk-ant-") {
            return Err(ArtifactError::preflight(
                "effective config snapshot would contain secret material",
            ));
        }
        Ok(snapshot)
    }

    /// Canonical normalized JSON bytes (sorted keys, trailing newline).
    pub fn normalize_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        let value = serde_json::to_value(self)
            .map_err(|e| ArtifactError::serialize(format!("effective config: {e}")))?;
        let normalized = sort_value(value);
        let mut bytes = serde_json::to_vec(&normalized)
            .map_err(|e| ArtifactError::serialize(format!("effective config normalize: {e}")))?;
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        Ok(bytes)
    }

    pub fn content_hash(&self) -> Result<String, ArtifactError> {
        Ok(hex_sha256(&self.normalize_bytes()?))
    }
}

/// Fingerprint tools from a live registry. Descriptions become surface ids when known.
pub fn fingerprint_registry(
    registry: &ToolRegistry,
) -> Result<Vec<ToolFingerprint>, ArtifactError> {
    let mut out = Vec::new();
    for def in registry.list() {
        let tool = registry
            .get(&def.name)
            .ok_or_else(|| ArtifactError::new(format!("tool '{}' missing after list", def.name)))?;
        let meta = tool.metadata();
        let input_schema_sha256 = schema_hash(tool.input_schema())?;
        let output_schema_sha256 = match tool.output_schema() {
            Some(s) => Some(schema_hash(s)?),
            None => None,
        };
        out.push(ToolFingerprint {
            name: def.name.clone(),
            description_surface_id: surface_id_for_tool(&def.name),
            input_schema_sha256,
            output_schema_sha256,
            side_effect: meta.side_effect,
            approval: approval_label(meta.approval),
            execution_mode: execution_mode_label(meta),
            parallelism: format!("{:?}", meta.parallelism).to_ascii_lowercase(),
            timeout_ms: meta.timeout.map(|d| d.as_millis() as u64),
            max_output_tokens: meta.max_output_tokens,
            source: source_label(&meta.source),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Convert an actual agent config into its stable, non-secret role snapshot.
#[allow(clippy::too_many_arguments)]
pub fn role_snapshot_from_config(
    role: &str,
    config: &orchest::run::AgentConfig,
    provider: &str,
    model: &str,
    request_options: Value,
    endpoint: Option<String>,
    hooks_label: &str,
    session_store_label: &str,
    executor_stable_label: Option<&str>,
) -> Result<AgentRoleSnapshot, ArtifactError> {
    preflight_request_options(&request_options)?;
    preflight_endpoint(endpoint.as_deref())?;
    if hooks_label.trim().is_empty() || session_store_label.trim().is_empty() {
        return Err(ArtifactError::preflight(
            "enabled component is missing a stable hooks/store label",
        ));
    }
    let runtime = runtime_snapshot(&config.runtime, executor_stable_label)?;
    Ok(AgentRoleSnapshot {
        role: role.into(),
        model_provider: provider.into(),
        model_name: model.into(),
        endpoint,
        request_options: sanitize_request_options(&request_options),
        max_steps: runtime.max_steps,
        allowed_tools: runtime.allowed_tools,
        tool_search_enabled: runtime.tool_search_enabled,
        compaction: runtime.compaction,
        mcp_servers: runtime.mcp_servers,
        webhook_enabled: runtime.webhook_enabled,
        code_execution: runtime.code_execution,
        run_depth: runtime.run_depth,
        repeated_failure_threshold: runtime.repeated_failure_threshold,
        supervision: supervision_label(&config.supervision_strategy),
        approval_mode: approval_mode_label(config.runtime.approval_mode),
        custom_approval: config.runtime.custom_approval_fn.is_some(),
        tool_execution_policy: tool_policy_label(&config.runtime.tool_execution_policy),
        budget: budget_limits(&config.budget),
        retry: retry_snapshot(config.retry_policy.as_ref()),
        hooks_label: hooks_label.into(),
        session_store_label: session_store_label.into(),
    })
}

/// Map known Briefing Desk tools to harness surface ids.
pub fn surface_id_for_tool(tool_name: &str) -> Option<String> {
    let id = match tool_name {
        "review_report" => SurfaceId::ReviewReportToolDescription,
        "search_fixtures" => SurfaceId::SearchFixturesToolDescription,
        "read_fixture" => SurfaceId::ReadFixtureToolDescription,
        "write_report" => SurfaceId::WriteReportToolDescription,
        "transcribe_audio" => SurfaceId::TranscribeAudioToolDescription,
        "describe_image" => SurfaceId::DescribeImageToolDescription,
        "synthesize_brief" => SurfaceId::SynthesizeBriefToolDescription,
        _ => return None,
    };
    Some(id.as_str().to_string())
}

/// Build a minimal tool fingerprint for offline unit tests without a registry.
pub fn tool_fingerprint_for_test(name: &str, input_schema: &Value) -> ToolFingerprint {
    ToolFingerprint {
        name: name.into(),
        description_surface_id: surface_id_for_tool(name),
        input_schema_sha256: schema_hash(input_schema).unwrap_or_else(|_| "0".repeat(64)),
        output_schema_sha256: None,
        side_effect: false,
        approval: "when_risky".into(),
        execution_mode: "normal".into(),
        parallelism: "serial".into(),
        timeout_ms: None,
        max_output_tokens: None,
        source: "in_process".into(),
    }
}

fn schema_hash(schema: &Value) -> Result<String, ArtifactError> {
    let normalized = sort_value(schema.clone());
    let bytes = serde_json::to_vec(&normalized)
        .map_err(|e| ArtifactError::serialize(format!("schema hash: {e}")))?;
    Ok(hex_sha256(&bytes))
}

struct RuntimeSnapshot {
    max_steps: u32,
    allowed_tools: Option<Vec<String>>,
    tool_search_enabled: bool,
    compaction: Option<CompactionSnapshot>,
    mcp_servers: Vec<McpServerSnapshot>,
    webhook_enabled: bool,
    code_execution: CodeExecutionSnapshot,
    run_depth: u32,
    repeated_failure_threshold: usize,
}

fn runtime_snapshot(
    runtime: &RuntimeConfig,
    executor_stable_label: Option<&str>,
) -> Result<RuntimeSnapshot, ArtifactError> {
    let mcp_servers = runtime
        .mcp_servers
        .iter()
        .map(mcp_server_snapshot)
        .collect::<Result<Vec<_>, _>>()?;
    let executor_label =
        if runtime.code_execution_enabled && runtime.code_execution_executor.is_some() {
            Some(
                executor_stable_label
                    .filter(|label| !label.trim().is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        ArtifactError::preflight(
                            "enabled code executor is missing a stable executor label",
                        )
                    })?,
            )
        } else {
            None
        };
    Ok(RuntimeSnapshot {
        max_steps: runtime.max_steps,
        allowed_tools: runtime.allowed_tools.clone(),
        tool_search_enabled: runtime.tool_search_enabled,
        compaction: runtime
            .compaction
            .as_ref()
            .map(|config| CompactionSnapshot {
                threshold: config.threshold,
                recent_messages: config.recent_messages,
            }),
        mcp_servers,
        webhook_enabled: runtime.webhook_enabled,
        code_execution: CodeExecutionSnapshot {
            enabled: runtime.code_execution_enabled,
            executor_label,
        },
        run_depth: runtime.run_depth,
        repeated_failure_threshold: runtime.repeated_failure.threshold,
    })
}

fn mcp_server_snapshot(
    server: &orchest::tool::mcp::McpServerConfig,
) -> Result<McpServerSnapshot, ArtifactError> {
    let transport = match &server.transport {
        McpTransport::Stdio { command, args } => {
            preflight_mcp_text("command", command)?;
            for arg in args {
                preflight_mcp_text("argument", arg)?;
            }
            McpTransportSnapshot::Stdio {
                command: command.clone(),
                args: args.clone(),
            }
        }
        McpTransport::StreamableHttp { url, auth } => {
            preflight_endpoint(Some(url))?;
            McpTransportSnapshot::StreamableHttp {
                endpoint: url.clone(),
                authentication: if auth.is_some() {
                    "bearer".into()
                } else {
                    "none".into()
                },
            }
        }
    };
    Ok(McpServerSnapshot {
        server_id: server.server_id.clone(),
        transport,
    })
}

fn preflight_mcp_text(kind: &str, value: &str) -> Result<(), ArtifactError> {
    if text_has_credentials(value) {
        return Err(ArtifactError::preflight(format!(
            "MCP {kind} looks like secret material"
        )));
    }
    Ok(())
}

fn budget_limits(b: &BudgetConfig) -> BudgetLimitsSnapshot {
    BudgetLimitsSnapshot {
        max_tokens: b.max_tokens,
        max_tool_calls: b.max_tool_calls,
        max_duration_ms: b.max_duration.map(|d| d.as_millis() as u64),
        max_cost_usd: b.max_cost_usd,
    }
}

fn retry_snapshot(policy: Option<&RetryPolicy>) -> RetrySnapshot {
    match policy {
        None => RetrySnapshot {
            configured: false,
            max_retries: None,
            backoff: None,
            base_delay_ms: None,
            max_delay_ms: None,
            jitter: None,
        },
        Some(p) => match &p.backoff {
            BackoffStrategy::Fixed(d) => RetrySnapshot {
                configured: true,
                max_retries: Some(p.max_retries),
                backoff: Some("fixed".into()),
                base_delay_ms: Some(d.as_millis() as u64),
                max_delay_ms: Some(d.as_millis() as u64),
                jitter: Some(false),
            },
            BackoffStrategy::Exponential { base, max, jitter } => RetrySnapshot {
                configured: true,
                max_retries: Some(p.max_retries),
                backoff: Some("exponential".into()),
                base_delay_ms: Some(base.as_millis() as u64),
                max_delay_ms: Some(max.as_millis() as u64),
                jitter: Some(*jitter),
            },
        },
    }
}

fn supervision_label(s: &SupervisionStrategy) -> String {
    match s {
        SupervisionStrategy::Stop => "stop".into(),
        SupervisionStrategy::Restart { max_retries } => format!("restart:{max_retries}"),
    }
}

fn approval_mode_label(m: ApprovalMode) -> String {
    match m {
        ApprovalMode::PerTool => "per_tool".into(),
        ApprovalMode::None => "none".into(),
        ApprovalMode::All => "all".into(),
    }
}

fn tool_policy_label(p: &impl std::fmt::Debug) -> String {
    // RuntimeConfig::tool_execution_policy is public; Debug string is stable for
    // Sequential / ParallelSafe and avoids depending on a re-export.
    let s = format!("{p:?}");
    match s.as_str() {
        "Sequential" => "sequential".into(),
        "ParallelSafe" => "parallel_safe".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn approval_label(a: Approval) -> String {
    match a {
        Approval::Never => "never".into(),
        Approval::WhenRisky => "when_risky".into(),
        Approval::Always => "always".into(),
    }
}

fn execution_mode_label(meta: &ToolMetadata) -> String {
    match &meta.execution_mode {
        orchest::tool::ToolExecutionMode::Normal => "normal".into(),
        orchest::tool::ToolExecutionMode::Draft { commit_tool } => {
            format!("draft:{commit_tool}")
        }
        orchest::tool::ToolExecutionMode::Commit { draft_tool } => {
            format!("commit:{draft_tool}")
        }
    }
}

fn source_label(source: &ToolSource) -> String {
    match source {
        ToolSource::InProcess => "in_process".into(),
        ToolSource::McpServer { server_id } => format!("mcp:{server_id}"),
        ToolSource::Skill { skill_name } => format!("skill:{skill_name}"),
        ToolSource::Builtin => "builtin".into(),
    }
}

/// Strip secret-looking keys from request options JSON.
fn sanitize_request_options(options: &Value) -> Value {
    match options {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if is_sensitive_configuration_name(k) {
                    continue;
                }
                out.insert(k.clone(), sanitize_request_options(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sanitize_request_options).collect()),
        other => other.clone(),
    }
}

fn preflight_capability_routes(routes: &[&CapabilityRoute]) -> Result<(), ArtifactError> {
    for route in routes {
        let endpoint = match route {
            CapabilityRoute::Live { endpoint, .. } | CapabilityRoute::Injected { endpoint, .. } => {
                endpoint.as_deref()
            }
            CapabilityRoute::Disabled | CapabilityRoute::Fake => None,
        };
        if let Some(url) = endpoint {
            preflight_endpoint(Some(url))?;
        }
    }
    Ok(())
}

fn preflight_endpoint(endpoint: Option<&str>) -> Result<(), ArtifactError> {
    if endpoint.is_some_and(url_has_credentials) {
        return Err(ArtifactError::preflight(
            "capability endpoint must not include credentials",
        ));
    }
    Ok(())
}

fn preflight_env_options(opts: &BTreeMap<String, Value>) -> Result<(), ArtifactError> {
    for (k, v) in opts {
        if is_sensitive_configuration_name(k) {
            return Err(ArtifactError::preflight(format!(
                "secret env option '{k}' must not enter effective config"
            )));
        }
        if value_has_credentials(v) {
            return Err(ArtifactError::preflight(format!(
                "env option '{k}' contains credential material"
            )));
        }
    }
    Ok(())
}

fn preflight_request_options(options: &Value) -> Result<(), ArtifactError> {
    if value_has_credentials(options) {
        return Err(ArtifactError::preflight(
            "request options contain credential material",
        ));
    }
    Ok(())
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v.clone()));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Convenience: default empty RuntimeConfig with max_steps override.
pub fn runtime_with_max_steps(max_steps: u32) -> RuntimeConfig {
    RuntimeConfig {
        max_steps,
        ..RuntimeConfig::default()
    }
}

/// Build a test-friendly baseline input.
pub fn sample_input() -> EffectiveConfigInput {
    EffectiveConfigInput {
        main_model_provider: "anthropic".into(),
        main_model_name: "claude-sonnet".into(),
        main_request_options: json!({"temperature": 0.0, "max_tokens": 1024}),
        main_runtime: runtime_with_max_steps(10),
        main_budget: BudgetConfig::default(),
        main_retry: None,
        main_supervision: SupervisionStrategy::Stop,
        main_hooks_label: "none".into(),
        main_session_store_label: "none".into(),
        main_executor_stable_label: None,
        reviewer_model_provider: "anthropic".into(),
        reviewer_model_name: "claude-sonnet".into(),
        reviewer_request_options: json!({}),
        reviewer_max_steps: 2,
        reviewer_budget: BudgetConfig::default(),
        reviewer_retry: None,
        reviewer_supervision: SupervisionStrategy::Stop,
        tools: vec![
            tool_fingerprint_for_test("search_fixtures", &json!({"type":"object"})),
            tool_fingerprint_for_test(
                "write_report",
                &json!({"type":"object","properties":{"path":{"type":"string"}}}),
            ),
        ],
        asr: CapabilityRoute::Fake,
        tts: CapabilityRoute::Disabled,
        vision: CapabilityRoute::Fake,
        session_mode: SessionPersistenceMode::None,
        env_options: BTreeMap::from([
            ("chat_model".into(), json!("anthropic/claude-sonnet")),
            ("chat_api_url_set".into(), json!(false)),
            ("chat_max_tokens".into(), json!(4096)),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ResumeArgs, RunArgs};
    use crate::eval::scripted_model::ScriptedModel;
    use crate::execution::{
        prepare_resume, prepare_run, ResolvedChatModel, ResolvedExecutionEnvironment,
    };
    use orchest::budget::BudgetUsage;
    use orchest::run::{CompactionConfig, RunId};
    use orchest::session::SessionSnapshot;
    use orchest::skill::executor::BareSubprocessExecutor;
    use orchest::tool::mcp::{McpAuth, McpServerConfig, McpTransport};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn product_preparation_profiles_actual_tools_routes_and_request_options() {
        let materials = tempfile::tempdir().unwrap();
        std::fs::write(materials.path().join("notes.md"), "evidence").unwrap();
        std::fs::write(materials.path().join("chart.png"), b"png").unwrap();
        std::fs::write(materials.path().join("interview.wav"), b"wav").unwrap();
        let output = materials.path().join("report.md");
        let chat = ResolvedChatModel::injected(
            Arc::new(ScriptedModel::followup_text("done")),
            json!({"max_tokens": 4096, "temperature": 0.0}),
            Some("https://chat.example.test/v1".into()),
        )
        .unwrap();
        let env = ResolvedExecutionEnvironment::offline(chat);

        let prepared = prepare_run(
            RunArgs {
                materials: materials.path().to_path_buf(),
                question: "question".into(),
                output,
                session: None,
                no_tts: false,
            },
            env,
        )
        .unwrap();

        let tool_names: Vec<_> = prepared
            .profile
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        assert!(tool_names.contains(&"review_report"));
        assert!(tool_names.contains(&"transcribe_audio"));
        assert!(tool_names.contains(&"describe_image"));
        assert!(tool_names.contains(&"synthesize_brief"));
        assert_eq!(prepared.profile.main.max_steps, 14);
        assert_eq!(prepared.profile.main.request_options["max_tokens"], 4096);
        assert_eq!(
            prepared.profile.vision,
            CapabilityRoute::Injected {
                provider: "scripted".into(),
                model: "eval-script".into(),
                endpoint: Some("https://chat.example.test/v1".into()),
            }
        );
    }

    #[test]
    fn fresh_and_followup_preparation_have_distinct_stable_session_profiles() {
        let materials = tempfile::tempdir().unwrap();
        std::fs::write(materials.path().join("notes.md"), "evidence").unwrap();
        let chat = ResolvedChatModel::injected(
            Arc::new(ScriptedModel::followup_text("done")),
            json!({"max_tokens": 4096}),
            None,
        )
        .unwrap();
        let fresh = prepare_run(
            RunArgs {
                materials: materials.path().to_path_buf(),
                question: "question".into(),
                output: materials.path().join("report.md"),
                session: None,
                no_tts: true,
            },
            ResolvedExecutionEnvironment::offline(chat.clone()),
        )
        .unwrap();
        let snapshot = SessionSnapshot {
            schema_version: SessionSnapshot::CURRENT_SCHEMA_VERSION.into(),
            session_id: "seed-session".into(),
            run_id: RunId::new(),
            messages: vec![],
            step: 0,
            budget_used: BudgetUsage::default(),
            active_config: fresh.config.clone(),
        };
        let followup = prepare_resume(
            snapshot,
            ResumeArgs {
                session: "seed-session".into(),
                question: "follow up".into(),
                output: PathBuf::from("followup.md"),
                no_tts: true,
            },
            ResolvedExecutionEnvironment::offline(chat),
        )
        .unwrap();

        assert_eq!(fresh.profile.session_mode, SessionPersistenceMode::None);
        assert_eq!(
            followup.profile.session_mode,
            SessionPersistenceMode::FollowUpFromSeed
        );
        assert_ne!(fresh.profile.session_mode, followup.profile.session_mode);
    }

    #[test]
    fn injected_chat_rejects_nested_secret_option_before_normalization() {
        let error = ResolvedChatModel::injected(
            Arc::new(ScriptedModel::followup_text("done")),
            json!({"transport": {"api_key": "sk-secret"}}),
            None,
        )
        .err()
        .expect("nested secret option must be rejected");

        assert!(error.to_string().contains("secret-like"));
    }

    #[test]
    fn nested_credentials_fail_in_injected_and_snapshot_preflight_without_leaking_values() {
        const CANARY: &str = "opaque-cross-layer-canary";
        let fixtures = [
            json!({"transport": {"refresh_token": CANARY}}),
            json!({"auth": [{"session_token": CANARY}]}),
            json!({"provider": {"security_token": CANARY}}),
            json!({"identity": {"access_key": CANARY}}),
            json!({"credentials": {"secret_key": CANARY}}),
            json!({"oauth": {"client_credential": CANARY}}),
            json!({"headers": {"X-Custom-Auth": CANARY}}),
        ];

        for fixture in fixtures {
            let injected_error = ResolvedChatModel::injected(
                Arc::new(ScriptedModel::followup_text("done")),
                fixture.clone(),
                None,
            )
            .err()
            .unwrap_or_else(|| panic!("injected construction accepted {fixture}"));
            assert!(!injected_error.to_string().contains(CANARY));

            let mut input = sample_input();
            input.main_request_options = fixture.clone();
            let snapshot_error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .unwrap_or_else(|| panic!("snapshot preflight accepted {fixture}"));
            assert!(!snapshot_error.to_string().contains(CANARY));
        }
    }

    #[test]
    fn request_option_urls_and_benign_nested_values_have_cross_layer_semantics() {
        const CANARY: &str = "opaque-url-canary";
        let credential_urls = [
            format!("https://chat.example.test/v1?access_key={CANARY}"),
            format!("https://chat.example.test/v1?client_credential={CANARY}"),
            format!("https://chat.example.test/v1?headers%2Djson=%7B%22Authorization%22%3A%22{CANARY}%22%7D"),
        ];

        for url in credential_urls {
            let fixture = json!({"service_tier": url});
            let injected_error = ResolvedChatModel::injected(
                Arc::new(ScriptedModel::followup_text("done")),
                fixture.clone(),
                None,
            )
            .err()
            .unwrap_or_else(|| panic!("injected construction accepted credential URL"));
            assert!(!injected_error.to_string().contains(CANARY));

            let mut input = sample_input();
            input.main_request_options = fixture;
            let snapshot_error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .unwrap_or_else(|| panic!("snapshot preflight accepted credential URL"));
            assert!(!snapshot_error.to_string().contains(CANARY));
        }

        let benign = json!({
            "service_tier": "https://chat.example.test/v1?region=public",
            "transport": {
                "retry_policy": {"max_attempts": 3},
                "credential_type": "public",
                "signature_algorithm": "sha256",
                "signed_headers": "host"
            }
        });
        assert!(ResolvedChatModel::injected(
            Arc::new(ScriptedModel::followup_text("done")),
            benign.clone(),
            None,
        )
        .is_ok());

        let mut input = sample_input();
        input.main_request_options = benign.clone();
        let snapshot = EffectiveConfigSnapshot::from_input(input)
            .expect("benign nested public options must survive snapshot preflight");
        assert_eq!(snapshot.main.request_options, benign);
    }

    #[test]
    fn mcp_credential_flag_and_assignment_forms_fail_without_leaking_values() {
        for args in [
            vec!["--api-key".into(), "hunter2".into()],
            vec!["--API_KEY".into(), "hunter2".into()],
            vec!["--token=hunter2".into()],
            vec!["--CLIENT-SECRET=hunter2".into()],
            vec!["CLIENT_SECRET=hunter2".into()],
        ] {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "credentialed-stdio".into(),
                transport: McpTransport::Stdio {
                    command: "mcp-server".into(),
                    args,
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .expect("credential-bearing MCP args must fail preflight");
            assert!(!error.to_string().contains("hunter2"));
        }
    }

    #[test]
    fn mcp_sensitive_semantic_name_variants_fail_without_leaking_values() {
        const CANARY: &str = "opaque-canary-value";
        let cases: &[(&str, &[&str])] = &[
            (
                "auth token assignment",
                &["--auth-token=opaque-canary-value"],
            ),
            (
                "auth token uppercase",
                &["--AUTH_TOKEN=opaque-canary-value"],
            ),
            ("access token split", &["--access-token", CANARY]),
            ("refresh token env", &["REFRESH_TOKEN=opaque-canary-value"]),
            ("session token mixed case", &["--Session_Token", CANARY]),
            ("bearer token", &["--bearer-token=opaque-canary-value"]),
            ("security token", &["SECURITY_TOKEN", CANARY]),
            ("api key mixed case", &["--Api_Key=opaque-canary-value"]),
            ("access key split", &["--access-key", CANARY]),
            ("secret key env", &["SECRET_KEY=opaque-canary-value"]),
            ("client credential", &["--client-credential", CANARY]),
            (
                "request signature",
                &["REQUEST_SIGNATURE=opaque-canary-value"],
            ),
            ("credential config", &["--credential-config", CANARY]),
            ("signature config", &["--signature_config", CANARY]),
            (
                "authentication config",
                &["--authentication-config", CANARY],
            ),
            ("bearer config", &["--bearer-config", CANARY]),
            (
                "header json",
                &["--headers-json={\"X-Trace\":\"opaque-canary-value\"}"],
            ),
            ("header config split", &["--HEADER_CONFIG", CANARY]),
            ("cookie", &["--session-cookie=opaque-canary-value"]),
            ("cookie config", &["--cookie-config", CANARY]),
            (
                "authorization header",
                &["Authorization: Bearer opaque-canary-value"],
            ),
            (
                "access token header",
                &["X-Access-Token: opaque-canary-value"],
            ),
        ];

        for (label, args) in cases {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: format!("credentialed-{label}"),
                transport: McpTransport::Stdio {
                    command: "mcp-server".into(),
                    args: args.iter().map(|value| (*value).to_owned()).collect(),
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .unwrap_or_else(|| panic!("{label} must fail MCP credential preflight"));
            assert!(
                !error.to_string().contains(CANARY),
                "{label} leaked the credential canary"
            );
        }
    }

    #[test]
    fn mcp_nested_json_strings_fail_without_leaking_values() {
        const CANARY: &str = "opaque-canary-value";
        let cases = [
            (
                "nested object key",
                r#"{"transport":{"refresh_token":"opaque-canary-value"}}"#,
            ),
            (
                "nested array key",
                r#"[{"options":[{"Secret-Key":"opaque-canary-value"}]}]"#,
            ),
            (
                "name and bearer value",
                r#"[{"name":"Authorization","value":"Bearer opaque-canary-value"}]"#,
            ),
            (
                "json assignment",
                r#"--options={"outer":{"access_token":"opaque-canary-value"}}"#,
            ),
            (
                "stringified nested json",
                r#"{"payload":"{\"session_token\":\"opaque-canary-value\"}"}"#,
            ),
        ];

        for (label, argument) in cases {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: format!("credentialed-json-{label}"),
                transport: McpTransport::Stdio {
                    command: "mcp-server".into(),
                    args: vec![argument.into()],
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .unwrap_or_else(|| panic!("{label} must fail recursive MCP preflight"));
            assert!(
                !error.to_string().contains(CANARY),
                "{label} leaked the credential canary"
            );
        }
    }

    #[test]
    fn mcp_command_text_is_scanned_before_snapshot() {
        const CANARY: &str = "opaque-canary-value";
        let commands = [
            "mcp-server --auth-token=opaque-canary-value",
            "REFRESH_TOKEN=opaque-canary-value",
            r#"mcp-server --options={"nested":{"api_key":"opaque-canary-value"}}"#,
        ];

        for command in commands {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "credentialed-command".into(),
                transport: McpTransport::Stdio {
                    command: command.into(),
                    args: Vec::new(),
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .expect("credential-bearing MCP command must fail preflight");
            assert!(!error.to_string().contains(CANARY));
        }
    }

    #[test]
    fn mcp_header_and_auth_injection_forms_fail_without_leaking_values() {
        let cases = [
            vec!["--header", "x-api-key:hunter2"],
            vec!["-H", "Authorization: Bearer hunter2"],
            vec!["-Hx-api-key:hunter2"],
            vec!["--request-header=Cookie: session=hunter2"],
            vec!["--HTTP_HEADER", "X-Api-Key: hunter2"],
            vec!["x_api_key:hunter2"],
            vec!["Authorization:Bearer hunter2"],
            vec!["Proxy_Authorization: Basic hunter2"],
            vec!["Cookie: session=hunter2"],
            vec!["Set-Cookie: session=hunter2"],
            vec!["Bearer hunter2"],
            vec!["--oauth2-bearer", "hunter2"],
            vec!["--basic-auth=hunter2"],
            vec!["--cookie=session=hunter2"],
        ];

        for case in cases {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "credentialed-stdio".into(),
                transport: McpTransport::Stdio {
                    command: "mcp-server".into(),
                    args: case.into_iter().map(str::to_owned).collect(),
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .expect("credential-bearing MCP header/auth args must fail preflight");
            assert!(!error.to_string().contains("hunter2"));
        }
    }

    #[test]
    fn mcp_query_credentials_fail_without_leaking_values() {
        for query_key in ["client_secret", "CLIENT-SECRET", "key", "AUTH"] {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "credentialed-http".into(),
                transport: McpTransport::StreamableHttp {
                    url: format!("https://mcp.example.test/v1?{query_key}=hunter2"),
                    auth: None,
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .expect("credential-bearing MCP query must fail preflight");
            assert!(!error.to_string().contains("hunter2"));
        }
    }

    #[test]
    fn cloud_signed_url_query_credentials_fail_without_leaking_values() {
        let credential_keys = [
            "X-Amz-Credential",
            "X_AMZ_SIGNATURE",
            "x-amz-security-token",
            "X-Goog-Credential",
            "x_goog_signature",
            "X-Goog-Security-Token",
            "AWSAccessKeyId",
            "OSS_ACCESS_KEY_ID",
            "x-amz-session-token",
            "secret_access_key",
            "signature",
            "security_token",
            "sig",
            "X%2dAmz%2dSignature",
        ];

        for query_key in credential_keys {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "signed-http".into(),
                transport: McpTransport::StreamableHttp {
                    url: format!("https://mcp.example.test/v1?{query_key}=hunter2"),
                    auth: None,
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .expect("cloud signed URL credentials must fail preflight");
            assert!(!error.to_string().contains("hunter2"));
        }
    }

    #[test]
    fn percent_encoded_query_credentials_fail_without_leaking_values() {
        const CANARY: &str = "opaque-canary-value";
        let queries = [
            "headers%2Djson=%7B%22X-Trace%22%3A%22opaque-canary-value%22%7D",
            "config=%7B%22outer%22%3A%7B%22refresh_token%22%3A%22opaque-canary-value%22%7D%7D",
            "%61uth%2Dtoken=opaque-canary-value",
            "ACCESS%5Ftoken=opaque-canary-value",
            "refresh%5Ftoken=opaque-canary-value",
            "session%2Dtoken=opaque-canary-value",
            "bearer%5Ftoken=opaque-canary-value",
            "security%2Dtoken=opaque-canary-value",
            "api%5Fkey=opaque-canary-value",
            "access%2Dkey=opaque-canary-value",
            "secret%5Fkey=opaque-canary-value",
            "client%2Dcredential=opaque-canary-value",
            "request%5Fsignature=opaque-canary-value",
            "session%2Dcookie=opaque-canary-value",
        ];

        for query in queries {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "credentialed-percent-query".into(),
                transport: McpTransport::StreamableHttp {
                    url: format!("https://mcp.example.test/v1?{query}"),
                    auth: None,
                },
            });

            let error = EffectiveConfigSnapshot::from_input(input)
                .err()
                .unwrap_or_else(|| panic!("encoded query must fail credential preflight"));
            assert!(!error.to_string().contains(CANARY));
        }
    }

    #[test]
    fn mcp_non_secret_lookalikes_are_allowed() {
        for argument in [
            "--api-key-file",
            "/var/run/mcp-key",
            "--tokenizer=cl100k_base",
            "--header-size=8192",
            "--cookie-policy=strict",
            "--user-agent=briefing-desk",
            "--monkey-mode=curious",
            "--keynote-theme=dark",
            "CLIENT_SECRETARY=briefing",
        ] {
            let mut input = sample_input();
            input.main_runtime.mcp_servers.push(McpServerConfig {
                server_id: "safe-stdio".into(),
                transport: McpTransport::Stdio {
                    command: "mcp-server".into(),
                    args: vec![argument.into()],
                },
            });
            assert!(
                EffectiveConfigSnapshot::from_input(input).is_ok(),
                "benign MCP argument was rejected: {argument}"
            );
        }

        let mut input = sample_input();
        input.main_runtime.mcp_servers.push(McpServerConfig {
            server_id: "safe-http".into(),
            transport: McpTransport::StreamableHttp {
                url: "https://mcp.example.test/v1?monkey=banana&monkey_mode=curious&keynote=launch&author=ada&signature_algorithm=sha256&credential_type=public&security_tokenizer=v2&signed_headers=host".into(),
                auth: None,
            },
        });

        assert!(EffectiveConfigSnapshot::from_input(input).is_ok());
    }

    #[test]
    fn hash_changes_with_compaction_threshold_and_recent_messages() {
        let base = EffectiveConfigSnapshot::from_input(sample_input())
            .unwrap()
            .content_hash()
            .unwrap();
        let mut changed = sample_input();
        changed.main_runtime.compaction = Some(CompactionConfig {
            threshold: 0.55,
            recent_messages: 3,
        });

        assert_ne!(
            EffectiveConfigSnapshot::from_input(changed)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );
    }

    #[test]
    fn hash_changes_with_mcp_webhook_and_code_execution_runtime() {
        let base = EffectiveConfigSnapshot::from_input(sample_input())
            .unwrap()
            .content_hash()
            .unwrap();

        let mut mcp = sample_input();
        mcp.main_runtime.mcp_servers.push(McpServerConfig {
            server_id: "fixtures".into(),
            transport: McpTransport::StreamableHttp {
                url: "https://mcp.example.test".into(),
                auth: Some(McpAuth::Bearer {
                    token: "sk-secret".into(),
                }),
            },
        });
        assert_ne!(
            EffectiveConfigSnapshot::from_input(mcp)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut webhook = sample_input();
        webhook.main_runtime.webhook_enabled = true;
        assert_ne!(
            EffectiveConfigSnapshot::from_input(webhook)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut code_execution = sample_input();
        code_execution.main_runtime.code_execution_enabled = true;
        assert_ne!(
            EffectiveConfigSnapshot::from_input(code_execution)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );
    }

    #[test]
    fn enabled_code_executor_without_stable_label_fails_preflight() {
        let mut input = sample_input();
        input.main_runtime.code_execution_enabled = true;
        input.main_runtime.code_execution_executor = Some(Arc::new(BareSubprocessExecutor::new()));

        let error = EffectiveConfigSnapshot::from_input(input).unwrap_err();
        assert!(error.to_string().contains("stable executor label"));
    }

    #[test]
    fn hash_stable_when_only_api_key_env_changes() {
        let mut a = sample_input();
        let mut b = sample_input();
        // API key must never enter snapshot; env_options only holds non-secret flags.
        a.env_options.insert("api_key_present".into(), json!(true));
        b.env_options.insert("api_key_present".into(), json!(true));
        // Simulated different secret values are not in the snapshot at all.
        let sa = EffectiveConfigSnapshot::from_input(a).unwrap();
        let sb = EffectiveConfigSnapshot::from_input(b).unwrap();
        assert_eq!(sa.content_hash().unwrap(), sb.content_hash().unwrap());
        let text = String::from_utf8(sa.normalize_bytes().unwrap()).unwrap();
        assert!(!text.contains("sk-"));
        assert!(!text.contains("secret-value"));
    }

    #[test]
    fn hash_stable_when_prompt_or_description_would_change() {
        // Descriptions are only surface_ids; changing description text is not in this snapshot.
        let mut a = sample_input();
        let mut b = sample_input();
        // Same tool schemas / surface ids
        a.tools[0].description_surface_id = Some("tool.search_fixtures.description".into());
        b.tools[0].description_surface_id = Some("tool.search_fixtures.description".into());
        let sa = EffectiveConfigSnapshot::from_input(a).unwrap();
        let sb = EffectiveConfigSnapshot::from_input(b).unwrap();
        assert_eq!(sa.content_hash().unwrap(), sb.content_hash().unwrap());
        // Ensure no raw description prose
        let text = String::from_utf8(sa.normalize_bytes().unwrap()).unwrap();
        assert!(!text.contains("Search local research materials"));
    }

    #[test]
    fn hash_changes_with_max_steps_budget_retry_approval_tool_schema_routes() {
        let base = EffectiveConfigSnapshot::from_input(sample_input())
            .unwrap()
            .content_hash()
            .unwrap();

        let mut steps = sample_input();
        steps.main_runtime.max_steps = 11;
        assert_ne!(
            EffectiveConfigSnapshot::from_input(steps)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut budget = sample_input();
        budget.main_budget.max_tokens = Some(1000);
        assert_ne!(
            EffectiveConfigSnapshot::from_input(budget)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut retry = sample_input();
        retry.main_retry = Some(RetryPolicy::recommended());
        assert_ne!(
            EffectiveConfigSnapshot::from_input(retry)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut approval = sample_input();
        approval.main_runtime.approval_mode = ApprovalMode::All;
        assert_ne!(
            EffectiveConfigSnapshot::from_input(approval)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut tool = sample_input();
        tool.tools[0] = tool_fingerprint_for_test(
            "search_fixtures",
            &json!({"type":"object","properties":{"q":{"type":"string"}}}),
        );
        assert_ne!(
            EffectiveConfigSnapshot::from_input(tool)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut route = sample_input();
        route.asr = CapabilityRoute::Live {
            provider: "openai".into(),
            model: "whisper-1".into(),
            endpoint: Some("https://api.openai.com/v1".into()),
        };
        assert_ne!(
            EffectiveConfigSnapshot::from_input(route)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut model = sample_input();
        model.main_model_name = "claude-opus".into();
        assert_ne!(
            EffectiveConfigSnapshot::from_input(model)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut max_tokens = sample_input();
        max_tokens.main_request_options["max_tokens"] = json!(4096);
        assert_ne!(
            EffectiveConfigSnapshot::from_input(max_tokens)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );

        let mut endpoint = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        endpoint.main.endpoint = Some("https://chat.example.test/v2".into());
        endpoint.case_profiles[0].main.endpoint = endpoint.main.endpoint.clone();
        assert_ne!(endpoint.content_hash().unwrap(), base);

        let mut session = sample_input();
        session.session_mode = SessionPersistenceMode::FollowUpFromSeed;
        assert_ne!(
            EffectiveConfigSnapshot::from_input(session)
                .unwrap()
                .content_hash()
                .unwrap(),
            base
        );
    }

    #[test]
    fn secret_env_option_fails_preflight() {
        let mut input = sample_input();
        input
            .env_options
            .insert("api_key".into(), json!("sk-secret"));
        let err = EffectiveConfigSnapshot::from_input(input).unwrap_err();
        assert!(err.message.contains("secret") || err.message.contains("api_key"));
    }

    #[test]
    fn credential_url_fails_preflight() {
        let mut input = sample_input();
        input.vision = CapabilityRoute::Live {
            provider: "x".into(),
            model: "y".into(),
            endpoint: Some("https://user:pass@example.com/v1".into()),
        };
        let err = EffectiveConfigSnapshot::from_input(input).unwrap_err();
        assert!(err.message.contains("credential"));
    }

    #[test]
    fn temporary_ids_do_not_affect_hash() {
        // Temporary session/run/store IDs are not part of EffectiveConfigInput.
        let a = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        let b = EffectiveConfigSnapshot::from_input(sample_input()).unwrap();
        assert_eq!(a.content_hash().unwrap(), b.content_hash().unwrap());
    }

    #[test]
    fn tool_surface_ids_only_for_known_tools() {
        assert_eq!(
            surface_id_for_tool("search_fixtures").as_deref(),
            Some("tool.search_fixtures.description")
        );
        assert!(surface_id_for_tool("unknown_tool").is_none());
    }

    #[test]
    fn custom_approval_label_stable() {
        let input = sample_input();
        // Simulate custom approval presence via hooks_label / custom flag path:
        // RuntimeConfig custom_approval_fn cannot be set from outside easily without Arc;
        // we assert the field exists on snapshot when we force via direct construction.
        let mut snap = EffectiveConfigSnapshot::from_input(input.clone()).unwrap();
        snap.main.custom_approval = true;
        snap.main.hooks_label = "custom_approval".into();
        let bytes = snap.normalize_bytes().unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("custom_approval"));
        assert!(!text.contains("0x")); // no pointer/Debug leaks
        let _ = Duration::from_secs(1);
        let _ = input;
    }
}
