//! Typed Briefing Desk execution preparation shared by product and eval paths.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use orchest::model::{ModelAdapter, RequestOptions};
use orchest::run::{AgentConfig, RunInput};
use orchest::session::{SessionSnapshot, SessionStore, SqliteSessionStore};
use orchest::tool::registry::ToolRegistry;
use orchest::tool::ContextMode;
use orchest::tool::ToolError;
use orchest_protocol::{Asr, Tts};
use serde_json::{json, Value};

use crate::app::{DemoError, ResumeArgs, RunArgs};
use crate::eval::effective_config::{
    fingerprint_registry, role_snapshot_from_config, CapabilityRoute, CaseProfileSnapshot,
    SessionPersistenceMode,
};
use crate::harness;
use crate::media::{self, DescribeImageTool, SynthesizeBriefTool, TranscribeAudioTool};
use crate::tools::{ReadFixtureTool, SearchFixturesTool, WriteReportTool};

pub const CHAT_MODEL_ENV: &str = "BRIEFING_DESK_CHAT_MODEL";
const CHAT_API_KEY_ENV: &str = "BRIEFING_DESK_CHAT_API_KEY";
const CHAT_API_URL_ENV: &str = "BRIEFING_DESK_CHAT_API_URL";
const CHAT_MAX_TOKENS_ENV: &str = "BRIEFING_DESK_CHAT_MAX_TOKENS";
const ASR_PROVIDER_ENV: &str = "BRIEFING_DESK_ASR_PROVIDER";
const ASR_MODEL_ENV: &str = "BRIEFING_DESK_ASR_MODEL";
const ASR_API_KEY_ENV: &str = "BRIEFING_DESK_ASR_API_KEY";
const TTS_PROVIDER_ENV: &str = "BRIEFING_DESK_TTS_PROVIDER";
const TTS_MODEL_ENV: &str = "BRIEFING_DESK_TTS_MODEL";
const TTS_API_KEY_ENV: &str = "BRIEFING_DESK_TTS_API_KEY";
const TTS_VOICE_ENV: &str = "BRIEFING_DESK_TTS_VOICE";
const DEFAULT_MAX_TOKENS: u32 = 4096;
// The live demo routinely needs search, multimodal reads, reviewer correction,
// and the final write. Ten steps truncated a valid formal eval case before its
// terminal write, so keep a small but sufficient product/eval-shared ceiling.
const MAIN_MAX_STEPS: u32 = 14;

#[derive(Debug, Clone, Copy)]
enum ChatSource {
    Injected,
    Live,
}

/// Chat adapter plus every behavior-affecting, non-secret resolved value.
#[derive(Clone)]
pub struct ResolvedChatModel {
    pub adapter: Arc<dyn ModelAdapter>,
    pub provider: String,
    pub model: String,
    pub request_options: Value,
    pub endpoint: Option<String>,
    source: ChatSource,
}

impl ResolvedChatModel {
    pub fn injected(
        adapter: Arc<dyn ModelAdapter>,
        request_options: Value,
        endpoint: Option<String>,
    ) -> Result<Self, DemoError> {
        // Validate the caller's complete JSON before serde drops unknown fields.
        validate_public_config(&request_options, endpoint.as_deref())?;
        let request_options = normalize_request_options(request_options)?;
        validate_public_config(&request_options, endpoint.as_deref())?;
        Ok(Self {
            provider: adapter.provider_name().to_string(),
            model: adapter.model_name().to_string(),
            adapter,
            request_options,
            endpoint,
            source: ChatSource::Injected,
        })
    }

    pub fn from_env() -> Result<Self, DemoError> {
        let model_id = std::env::var(CHAT_MODEL_ENV).map_err(|_| {
            format!("no chat model configured: set {CHAT_MODEL_ENV} to a provider/model string")
        })?;
        let endpoint = std::env::var(CHAT_API_URL_ENV).ok();
        let max_tokens = std::env::var(CHAT_MAX_TOKENS_ENV)
            .ok()
            .map(|raw| {
                raw.trim()
                    .parse::<u32>()
                    .map_err(|e| format!("invalid {CHAT_MAX_TOKENS_ENV}: {e}"))
            })
            .transpose()?
            .unwrap_or(DEFAULT_MAX_TOKENS);
        let request_options = normalize_request_options(json!({"max_tokens": max_tokens}))?;
        validate_public_config(&request_options, endpoint.as_deref())?;
        let adapter =
            orchest_provider::create_adapter_from_config(orchest_provider::ProviderRuntimeConfig {
                model: model_id,
                api_key: std::env::var(CHAT_API_KEY_ENV).ok(),
                api_key_env: None,
                api_url: endpoint.clone(),
                max_tokens: Some(max_tokens),
            })
            .map_err(|e| format!("constructing chat model: {e}"))?;
        let adapter: Arc<dyn ModelAdapter> = Arc::from(adapter);
        Ok(Self {
            provider: adapter.provider_name().to_string(),
            model: adapter.model_name().to_string(),
            adapter,
            request_options,
            endpoint,
            source: ChatSource::Live,
        })
    }

    fn route(&self) -> CapabilityRoute {
        match self.source {
            ChatSource::Injected => CapabilityRoute::Injected {
                provider: self.provider.clone(),
                model: self.model.clone(),
                endpoint: self.endpoint.clone(),
            },
            ChatSource::Live => CapabilityRoute::Live {
                provider: self.provider.clone(),
                model: self.model.clone(),
                endpoint: self.endpoint.clone(),
            },
        }
    }
}

#[derive(Clone)]
enum ResolvedAsr {
    Fake,
    Live {
        provider: String,
        model: String,
        api_key: String,
    },
}

#[derive(Clone)]
enum ResolvedTts {
    Fake,
    Live {
        provider: String,
        model: String,
        api_key: String,
    },
}

/// Resolved capability choices. Secrets stay in execution-only variants.
#[derive(Clone)]
pub struct ResolvedExecutionEnvironment {
    pub chat: ResolvedChatModel,
    asr: ResolvedAsr,
    tts: ResolvedTts,
    /// Voice handed to `Tts::synthesize`. Every live dialect requires one.
    tts_voice: Option<String>,
}

impl ResolvedExecutionEnvironment {
    pub fn offline(chat: ResolvedChatModel) -> Self {
        Self {
            chat,
            asr: ResolvedAsr::Fake,
            tts: ResolvedTts::Fake,
            tts_voice: None,
        }
    }

    pub fn from_env() -> Result<Self, DemoError> {
        let chat = ResolvedChatModel::from_env()?;
        let asr = resolve_asr_env()?;
        let tts = resolve_tts_env()?;
        let tts_voice = std::env::var(TTS_VOICE_ENV)
            .ok()
            .map(|voice| voice.trim().to_string())
            .filter(|voice| !voice.is_empty());
        Ok(Self {
            chat,
            asr,
            tts,
            tts_voice,
        })
    }
}

/// Stable profile type shared with effective-config snapshots.
pub type ExecutionProfile = CaseProfileSnapshot;

pub struct PreparedRun {
    pub config: AgentConfig,
    pub input: RunInput,
    pub registry: ToolRegistry,
    pub profile: ExecutionProfile,
    pub output: PathBuf,
    pub no_tts: bool,
}

pub struct PreparedResume {
    pub config: AgentConfig,
    pub snapshot: SessionSnapshot,
    pub input: RunInput,
    pub registry: ToolRegistry,
    pub profile: ExecutionProfile,
    pub output: PathBuf,
    pub no_tts: bool,
    pub tts: Option<Box<dyn Tts>>,
    pub tts_voice: Option<String>,
}

pub fn prepare_run(
    args: RunArgs,
    env: ResolvedExecutionEnvironment,
) -> Result<PreparedRun, DemoError> {
    let corpus = media::discover(&args.materials)?;
    let text_entries = corpus
        .text
        .iter()
        .map(|path| {
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            Ok::<_, DemoError>((path.clone(), content))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let reviewer_config = build_agent_config(
        "briefing-reviewer",
        harness::REVIEWER_SYSTEM_PROMPT,
        2,
        &env.chat,
    )?;
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(SearchFixturesTool::new(text_entries)))?;
    registry.register(Arc::new(ReadFixtureTool::new(corpus.text.clone())))?;
    registry.register(reviewer_tool(&reviewer_config, &env.chat.adapter)?)?;

    let asr_route = if corpus.audio.is_empty() {
        CapabilityRoute::Disabled
    } else {
        let (adapter, route) = build_asr(&env.asr)?;
        registry.register(Arc::new(TranscribeAudioTool::new(
            corpus.audio.clone(),
            adapter,
        )))?;
        route
    };
    let vision_route = if corpus.images.is_empty() {
        CapabilityRoute::Disabled
    } else {
        registry.register(Arc::new(DescribeImageTool::new(
            corpus.images,
            Arc::clone(&env.chat.adapter),
        )))?;
        env.chat.route()
    };
    registry.register(Arc::new(WriteReportTool::new(args.output.clone())))?;
    let tts_route = if args.no_tts {
        CapabilityRoute::Disabled
    } else {
        let (adapter, route) = build_tts(&env.tts)?;
        registry.register(Arc::new(SynthesizeBriefTool::new(
            args.output.with_extension("wav"),
            adapter,
            env.tts_voice.clone(),
        )))?;
        route
    };

    let mut config = build_agent_config(
        "briefing-agent",
        harness::MAIN_SYSTEM_PROMPT,
        MAIN_MAX_STEPS,
        &env.chat,
    )?;
    let session_mode = if let Some(session_id) = &args.session {
        let store: Arc<dyn SessionStore> = Arc::new(open_session_store(session_id)?);
        config = config.with_session_store(store, session_id.clone());
        SessionPersistenceMode::FreshSqlite
    } else {
        SessionPersistenceMode::None
    };
    let profile = build_profile(
        &config,
        &reviewer_config,
        &registry,
        &env.chat,
        asr_route,
        tts_route,
        vision_route,
        session_mode,
    )?;
    Ok(PreparedRun {
        config,
        input: RunInput::text(args.question),
        registry,
        profile,
        output: args.output,
        no_tts: args.no_tts,
    })
}

pub fn prepare_resume(
    mut snapshot: SessionSnapshot,
    args: ResumeArgs,
    env: ResolvedExecutionEnvironment,
) -> Result<PreparedResume, DemoError> {
    apply_chat_config(&mut snapshot.active_config, &env.chat)?;
    let config = snapshot.active_config.clone();
    let reviewer_config = build_agent_config(
        "briefing-reviewer",
        harness::REVIEWER_SYSTEM_PROMPT,
        2,
        &env.chat,
    )?;
    let registry = ToolRegistry::new();
    let (tts, tts_route) = if args.no_tts {
        (None, CapabilityRoute::Disabled)
    } else {
        let (adapter, route) = build_tts(&env.tts)?;
        (Some(adapter), route)
    };
    let profile = build_profile(
        &config,
        &reviewer_config,
        &registry,
        &env.chat,
        CapabilityRoute::Disabled,
        tts_route,
        CapabilityRoute::Disabled,
        SessionPersistenceMode::FollowUpFromSeed,
    )?;
    Ok(PreparedResume {
        config,
        snapshot,
        input: RunInput::text(args.question),
        registry,
        profile,
        output: args.output,
        no_tts: args.no_tts,
        tts,
        tts_voice: env.tts_voice.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_profile(
    config: &AgentConfig,
    reviewer_config: &AgentConfig,
    registry: &ToolRegistry,
    chat: &ResolvedChatModel,
    asr: CapabilityRoute,
    tts: CapabilityRoute,
    vision: CapabilityRoute,
    session_mode: SessionPersistenceMode,
) -> Result<ExecutionProfile, DemoError> {
    let main_store = if config.session_store.is_some() {
        "sqlite"
    } else {
        "none"
    };
    Ok(CaseProfileSnapshot {
        case_ids: Vec::new(),
        main: role_snapshot_from_config(
            "main",
            config,
            &chat.provider,
            &chat.model,
            chat.request_options.clone(),
            chat.endpoint.clone(),
            "none",
            main_store,
            None,
        )?,
        reviewer: role_snapshot_from_config(
            "reviewer",
            reviewer_config,
            &chat.provider,
            &chat.model,
            chat.request_options.clone(),
            chat.endpoint.clone(),
            "none",
            "none",
            None,
        )?,
        tools: fingerprint_registry(registry)?,
        asr,
        tts,
        vision,
        session_mode,
    })
}

fn build_agent_config(
    name: &str,
    prompt: &str,
    max_steps: u32,
    chat: &ResolvedChatModel,
) -> Result<AgentConfig, DemoError> {
    let mut config = AgentConfig::builder(name, chat.model.clone())
        .system_prompt(prompt)
        .max_steps(max_steps)
        .build()
        .map_err(|e| format!("building {name} config: {e}"))?;
    apply_chat_config(&mut config, chat)?;
    Ok(config)
}

fn apply_chat_config(config: &mut AgentConfig, chat: &ResolvedChatModel) -> Result<(), DemoError> {
    let options: RequestOptions = serde_json::from_value(chat.request_options.clone())
        .map_err(|e| format!("invalid resolved chat request options: {e}"))?;
    config.model.spec.provider = chat.provider.clone();
    config.model.spec.model = chat.model.clone();
    config.model.spec.api_url = chat.endpoint.clone();
    config.model.spec.max_tokens = options.max_tokens;
    config.model.options = options;
    Ok(())
}

fn normalize_request_options(overrides: Value) -> Result<Value, DemoError> {
    let mut resolved = serde_json::to_value(RequestOptions::default())
        .map_err(|e| format!("serializing default chat request options: {e}"))?;
    let (Some(defaults), Some(overrides)) = (resolved.as_object_mut(), overrides.as_object())
    else {
        return Err("resolved chat request options must be a JSON object".into());
    };
    defaults.extend(overrides.clone());
    let parsed: RequestOptions = serde_json::from_value(resolved)
        .map_err(|e| format!("invalid resolved chat request options: {e}"))?;
    serde_json::to_value(parsed)
        .map_err(|e| format!("serializing resolved chat request options: {e}").into())
}

fn reviewer_tool(
    reviewer_config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
) -> Result<Arc<dyn orchest::tool::Tool>, DemoError> {
    let tool = reviewer_config
        .as_tool("review_report", harness::REVIEW_REPORT_TOOL_DESCRIPTION)
        .model(Arc::clone(model))
        .registry(ToolRegistry::new())
        .context_mode(ContextMode::Fresh)
        // The advertised schema must agree with `input_mapper` and with the
        // tool description, both of which name `draft`; `SubAgentBuilder`'s
        // default schema is `{"input": "string"}`, which a live model follows
        // to the letter and the mapper then rejects.
        .input_schema(json!({
            "type": "object",
            "properties": {
                "draft": {
                    "type": "string",
                    "description": "full draft Markdown to review"
                }
            },
            "required": ["draft"]
        }))
        .input_mapper(|input: Value| {
            input
                .get("draft")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ToolError::fatal("missing required parameter 'draft'"))
        })
        .output_extractor(
            |details: Value| json!({"output": details.get("output").cloned().unwrap_or(details)}),
        )
        .build()
        .map_err(|e| format!("building reviewer tool: {e}"))?;
    Ok(tool)
}

fn resolve_asr_env() -> Result<ResolvedAsr, DemoError> {
    resolve_triplet(ASR_PROVIDER_ENV, ASR_MODEL_ENV, ASR_API_KEY_ENV).map(|value| match value {
        Some((provider, model, api_key)) => ResolvedAsr::Live {
            provider,
            model,
            api_key,
        },
        None => ResolvedAsr::Fake,
    })
}

fn resolve_tts_env() -> Result<ResolvedTts, DemoError> {
    resolve_triplet(TTS_PROVIDER_ENV, TTS_MODEL_ENV, TTS_API_KEY_ENV).map(|value| match value {
        Some((provider, model, api_key)) => ResolvedTts::Live {
            provider,
            model,
            api_key,
        },
        None => ResolvedTts::Fake,
    })
}

fn resolve_triplet(
    provider_env: &str,
    model_env: &str,
    key_env: &str,
) -> Result<Option<(String, String, String)>, DemoError> {
    let values = [
        std::env::var(provider_env).ok(),
        std::env::var(model_env).ok(),
        std::env::var(key_env).ok(),
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let [Some(provider), Some(model), Some(key)] = values else {
        return Err(
            format!("{provider_env}, {model_env}, and {key_env} must be set together").into(),
        );
    };
    Ok(Some((provider, model, key)))
}

fn build_asr(value: &ResolvedAsr) -> Result<(Box<dyn Asr>, CapabilityRoute), DemoError> {
    match value {
        ResolvedAsr::Fake => Ok((Box::new(media::fake_asr()), CapabilityRoute::Fake)),
        ResolvedAsr::Live {
            provider,
            model,
            api_key,
        } => Ok((
            media::live_asr(provider, model, api_key)?,
            CapabilityRoute::Live {
                provider: provider.clone(),
                model: model.clone(),
                endpoint: None,
            },
        )),
    }
}

fn build_tts(value: &ResolvedTts) -> Result<(Box<dyn Tts>, CapabilityRoute), DemoError> {
    match value {
        ResolvedTts::Fake => Ok((Box::new(media::fake_tts()), CapabilityRoute::Fake)),
        ResolvedTts::Live {
            provider,
            model,
            api_key,
        } => Ok((
            media::live_tts(provider, model, api_key)?,
            CapabilityRoute::Live {
                provider: provider.clone(),
                model: model.clone(),
                endpoint: None,
            },
        )),
    }
}

pub fn open_session_store(session_id: &str) -> Result<SqliteSessionStore, DemoError> {
    let path = session_db_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    SqliteSessionStore::open(&path)
        .map_err(|e| format!("opening session store {}: {e}", path.display()).into())
}

pub fn session_db_path(session_id: &str) -> PathBuf {
    PathBuf::from(".briefing-desk-sessions").join(format!("{session_id}.sqlite3"))
}

fn validate_public_config(options: &Value, endpoint: Option<&str>) -> Result<(), DemoError> {
    if endpoint.is_some_and(crate::eval::credential::url_has_credentials) {
        return Err("chat endpoint must not contain credentials".into());
    }
    if crate::eval::credential::value_has_credentials(options) {
        return Err("chat request options contain secret-like fields".into());
    }
    Ok(())
}

pub fn resolved_env_options(chat: &ResolvedChatModel) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "chat_model".into(),
            json!(format!("{}/{}", chat.provider, chat.model)),
        ),
        ("chat_endpoint".into(), json!(chat.endpoint)),
        ("chat_request_options".into(), chat.request_options.clone()),
    ])
}
