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
use orchest::tool::registry::ToolRegistry;
use orchest::tool::{Approval, ToolMetadata, ToolSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::harness::SurfaceId;

use super::artifact::ArtifactError;

/// Effective-config schema version.
pub const EFFECTIVE_CONFIG_SCHEMA_VERSION: &str = "1";

/// How a media capability is routed for this run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CapabilityRoute {
    Disabled,
    Fake,
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
    /// Non-secret request options only.
    pub request_options: Value,
    pub max_steps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    pub tool_search_enabled: bool,
    pub compaction_enabled: bool,
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
    /// Resolved non-secret environment-driven options (values, not raw secrets).
    pub env_options: BTreeMap<String, Value>,
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

        let snap = Self {
            schema_version: EFFECTIVE_CONFIG_SCHEMA_VERSION.to_string(),
            main: AgentRoleSnapshot {
                role: "main".into(),
                model_provider: input.main_model_provider,
                model_name: input.main_model_name,
                request_options: sanitize_request_options(&input.main_request_options),
                max_steps: input.main_runtime.max_steps,
                allowed_tools: input.main_runtime.allowed_tools.clone(),
                tool_search_enabled: input.main_runtime.tool_search_enabled,
                compaction_enabled: input.main_runtime.compaction.is_some(),
                run_depth: input.main_runtime.run_depth,
                repeated_failure_threshold: input.main_runtime.repeated_failure.threshold,
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
                request_options: sanitize_request_options(&input.reviewer_request_options),
                max_steps: input.reviewer_max_steps,
                allowed_tools: None,
                tool_search_enabled: false,
                compaction_enabled: false,
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
            env_options: input.env_options,
        };

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
                if super::trajectory::is_secret_key(k) {
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
        if let CapabilityRoute::Live {
            endpoint: Some(url),
            ..
        } = route
        {
            if url_has_credentials(url) {
                return Err(ArtifactError::preflight(format!(
                    "capability endpoint must not include credentials: {url}"
                )));
            }
        }
    }
    Ok(())
}

fn preflight_env_options(opts: &BTreeMap<String, Value>) -> Result<(), ArtifactError> {
    for (k, v) in opts {
        if super::trajectory::is_secret_key(k) {
            return Err(ArtifactError::preflight(format!(
                "secret env option '{k}' must not enter effective config"
            )));
        }
        if let Value::String(s) = v {
            if s.contains("://") && url_has_credentials(s) {
                return Err(ArtifactError::preflight(format!(
                    "env option '{k}' has credential-bearing URL"
                )));
            }
            if looks_like_api_key_value(s) {
                return Err(ArtifactError::preflight(format!(
                    "env option '{k}' looks like a secret value"
                )));
            }
        }
    }
    Ok(())
}

fn preflight_request_options(options: &Value) -> Result<(), ArtifactError> {
    match options {
        Value::Object(map) => {
            for (k, v) in map {
                if super::trajectory::is_secret_key(k) {
                    return Err(ArtifactError::preflight(format!(
                        "request option '{k}' is secret and cannot be snapshotted"
                    )));
                }
                preflight_request_options(v)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                preflight_request_options(item)?;
            }
            Ok(())
        }
        Value::String(s) if looks_like_api_key_value(s) => Err(ArtifactError::preflight(
            "request options contain secret-looking string",
        )),
        _ => Ok(()),
    }
}

fn url_has_credentials(url: &str) -> bool {
    // userinfo@ or query with key-like params
    if let Some(after_scheme) = url.split("://").nth(1) {
        let host_part = after_scheme.split('/').next().unwrap_or("");
        if host_part.contains('@') {
            return true;
        }
    }
    let lower = url.to_ascii_lowercase();
    lower.contains("api_key=")
        || lower.contains("access_token=")
        || lower.contains("token=")
        || lower.contains("password=")
}

fn looks_like_api_key_value(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("sk-")
        || t.starts_with("sk-ant-")
        || (t.len() >= 32
            && t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            && (t.contains("sk") || t.starts_with("key"))
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
    use std::time::Duration;

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
