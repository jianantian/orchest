use std::env;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use agent_runtime_core::budget::BudgetConfig;
use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::{
    create_adapter_from_config, normalize_provider_model, ModelSpec, ProviderRuntimeConfig,
    RequestOptions,
};
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::agent::AgentTool;
use agent_runtime_core::tool::builtin::WriteFileTool;
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_core::tool::{
    JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::{json, Value};

type ExampleAgentParts = (
    AgentConfig,
    Arc<dyn agent_runtime_core::model::ModelAdapter>,
    ToolRegistry,
);

const EXA_SEARCH_URL: &str = "https://api.exa.ai/search";
const DEFAULT_REPORT_PATH: &str = "target/deep-research-report.md";
const DEFAULT_MIN_RESEARCH_CALLS: u32 = 6;
const MAX_HIGHLIGHT_CHARS: usize = 900;
const WEB_SEARCH_SYSTEM_PROMPT: &str =
    include_str!("../support/deep_research_prompts/web_search_system.md");
const MAIN_SYSTEM_PROMPT: &str = include_str!("../support/deep_research_prompts/main_system.md");
const RESEARCH_INSTRUCTIONS_PROMPT: &str =
    include_str!("../support/deep_research_prompts/research_instructions.md");

fn require_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    env::var(name).map_err(|_| format!("{name} is required for this non-mock example").into())
}

fn provider_url() -> Option<String> {
    env::var("ANTHROPIC_API_URL").ok()
}

fn current_date_label() -> String {
    // Avoid a date dependency in the example. The date is used only to anchor
    // freshness instructions, and the shell date command is available on the
    // supported local development platforms for these examples.
    std::process::Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "today".to_string())
}

fn prompt_template(
    name: &str,
    values: &[(&str, String)],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut prompt = match name {
        "web_search_system" => WEB_SEARCH_SYSTEM_PROMPT,
        "main_system" => MAIN_SYSTEM_PROMPT,
        "research_instructions" => RESEARCH_INSTRUCTIONS_PROMPT,
        _ => return Err(format!("missing prompt template: {name}").into()),
    }
    .trim()
    .to_string();
    for (key, value) in values {
        prompt = prompt.replace(&format!("{{{{{key}}}}}"), value);
    }
    Ok(prompt)
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_supported_exa_category(value: &str) -> bool {
    matches!(
        value,
        "company" | "people" | "research paper" | "news" | "personal site" | "financial report"
    )
}

#[derive(Debug)]
struct ExaSearchTool {
    metadata: ToolMetadata,
    input_schema: JsonSchema,
}

impl ExaSearchTool {
    fn new() -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: false,
                requires_approval: false,
                cost_hint: None,
                timeout: Some(Duration::from_secs(30)),
                max_output_tokens: None,
                source: ToolSource::InProcess,
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "rationale": { "type": "string" },
                    "category": { "type": "string" },
                    "include_domains": { "type": "string" },
                    "start_published_date": { "type": "string" }
                },
                "required": ["query"]
            }),
        }
    }
}

#[async_trait]
impl Tool for ExaSearchTool {
    fn name(&self) -> &str {
        "exa_search"
    }

    fn description(&self) -> &str {
        "Search the web with Exa highlights for agent workflows."
    }

    fn input_schema(&self) -> &JsonSchema {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError {
                message: "missing required parameter 'query'".into(),
                code: Some("MISSING_PARAM".into()),
            })?;
        let rationale = input.get("rationale").and_then(Value::as_str).unwrap_or("");
        let category = input.get("category").and_then(Value::as_str).unwrap_or("");
        let include_domains = input
            .get("include_domains")
            .and_then(Value::as_str)
            .unwrap_or("");
        let start_published_date = input
            .get("start_published_date")
            .and_then(Value::as_str)
            .unwrap_or("");

        let mut payload = json!({
            "query": query,
            "type": env::var("EXA_SEARCH_TYPE").unwrap_or_else(|_| "auto".to_string()),
            "numResults": env::var("EXA_NUM_RESULTS")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(5),
            "contents": { "highlights": true }
        });
        if !category.is_empty() {
            if !is_supported_exa_category(category) {
                return Err(ToolError {
                    message: format!("unsupported Exa category: {category}"),
                    code: Some("BAD_CATEGORY".into()),
                });
            }
            payload["category"] = Value::String(category.to_string());
        }
        if !include_domains.is_empty() {
            payload["includeDomains"] = json!(split_csv(include_domains));
        }
        if !start_published_date.is_empty() {
            payload["startPublishedDate"] = Value::String(start_published_date.to_string());
        }
        if env::var("EXA_LIVECRAWL").ok().as_deref() == Some("1") {
            payload["contents"]["maxAgeHours"] = Value::from(0);
        }

        let response = reqwest::Client::new()
            .post(EXA_SEARCH_URL)
            .header("Content-Type", "application/json")
            .header(
                "x-api-key",
                env::var("EXA_API_KEY").map_err(|_| ToolError {
                    message: "EXA_API_KEY is required for this non-mock example".into(),
                    code: Some("MISSING_ENV".into()),
                })?,
            )
            .json(&payload)
            .send()
            .await
            .map_err(|e| ToolError {
                message: format!("Exa request failed: {e}"),
                code: Some("EXA_REQUEST".into()),
            })?;
        let status = response.status();
        let data: Value = response.json().await.map_err(|e| ToolError {
            message: format!("failed to parse Exa response: {e}"),
            code: Some("EXA_RESPONSE".into()),
        })?;
        if !status.is_success() {
            return Err(ToolError {
                message: format!("Exa HTTP {status}: {data}"),
                code: Some("EXA_HTTP".into()),
            });
        }

        let results = data
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mapped_results: Vec<Value> = results
            .iter()
            .map(|result| {
                let evidence = result
                    .get("highlights")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                json!({
                    "title": result.get("title").cloned().unwrap_or(Value::Null),
                    "url": result.get("url").cloned().unwrap_or(Value::Null),
                    "published_date": result.get("publishedDate").cloned().unwrap_or(Value::Null),
                    "author": result.get("author").cloned().unwrap_or(Value::Null),
                    "evidence": evidence.chars().take(MAX_HIGHLIGHT_CHARS).collect::<String>()
                })
            })
            .collect();

        Ok(ToolOutput::Immediate(json!({
            "provider": "exa",
            "query": query,
            "rationale": rationale,
            "category": if category.is_empty() { Value::Null } else { Value::String(category.to_string()) },
            "include_domains": if include_domains.is_empty() { Value::Null } else { json!(split_csv(include_domains)) },
            "start_published_date": if start_published_date.is_empty() { Value::Null } else { Value::String(start_published_date.to_string()) },
            "search_type": data.get("searchType").cloned().unwrap_or(Value::Null),
            "request_id": data.get("requestId").cloned().unwrap_or(Value::Null),
            "cost_dollars": data.get("costDollars").cloned().unwrap_or(Value::Null),
            "result_count": mapped_results.len(),
            "results": mapped_results
        })))
    }
}

fn agent_config(
    model_ref: &str,
    system_prompt: String,
    max_steps: u32,
) -> Result<AgentConfig, Box<dyn std::error::Error>> {
    let normalized = normalize_provider_model(model_ref)?;
    Ok(AgentConfig {
        system_prompt,
        model: ModelSpec {
            provider: normalized.provider.into(),
            model: normalized.model.into(),
            api_key_env: None,
            api_url: provider_url(),
            max_tokens: None,
            context_window_size: None,
        },
        request_options: RequestOptions::default(),
        budget: BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        },
        max_steps,
        allowed_skills: None,
        allowed_tools: None,
        mcp_servers: vec![],
        tool_search_enabled: false,
        compaction_threshold: None,
        compaction_recent_messages: 10,
        webhook_enabled: false,
        code_execution_enabled: false,
        skills_dir: None,
        run_depth: 0,
    })
}

fn provider_model(
    model_ref: String,
) -> Result<Arc<dyn agent_runtime_core::model::ModelAdapter>, Box<dyn std::error::Error>> {
    let adapter = create_adapter_from_config(ProviderRuntimeConfig {
        model: model_ref,
        api_key: None,
        api_key_env: None,
        api_url: provider_url(),
        max_tokens: None,
    })?;
    Ok(Arc::from(adapter))
}

fn build_web_search_agent() -> Result<ExampleAgentParts, Box<dyn std::error::Error>> {
    let today = current_date_label();
    let model_ref = require_env("WEB_SEARCH_MODEL")?;
    let config = agent_config(
        &model_ref,
        prompt_template("web_search_system", &[("today", today)])?,
        10,
    )?;
    let model = provider_model(model_ref)?;
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ExaSearchTool::new()))?;
    Ok((config, model, registry))
}

fn build_deep_research_agent(
    web_config: AgentConfig,
    web_model: Arc<dyn agent_runtime_core::model::ModelAdapter>,
    web_registry: ToolRegistry,
    report_path: &str,
    min_calls: u32,
) -> Result<ExampleAgentParts, Box<dyn std::error::Error>> {
    let today = current_date_label();
    let model_ref = require_env("DEEP_RESEARCH_MODEL")?;
    let config = agent_config(
        &model_ref,
        prompt_template(
            "main_system",
            &[
                ("today", today),
                ("report_path", report_path.to_string()),
                ("min_calls", min_calls.to_string()),
            ],
        )?,
        40,
    )?;
    let model = provider_model(model_ref)?;
    let mut registry = ToolRegistry::new();
    let input_schema = json!({
        "type": "object",
        "properties": {
            "question": {
                "type": "string",
                "description": "Task or question to delegate to the child agent"
            }
        },
        "required": ["question"]
    });
    registry.register(Arc::new(AgentTool::new(
        "web_research".into(),
        "Delegate web research to an isolated web-search sub-agent.".into(),
        input_schema,
        web_config,
        web_model,
        web_registry,
        Arc::new(|value| {
            value
                .get("question")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ToolError {
                    message: "missing required parameter 'question'".into(),
                    code: Some("MISSING_PARAM".into()),
                })
        }),
        Arc::new(|details| details.get("output").cloned().unwrap_or(details)),
    )))?;
    registry.register(Arc::new(WriteFileTool::new_with_approval(false)))?;
    Ok((config, model, registry))
}

fn research_instructions(
    question: &str,
    report_path: &str,
    min_calls: u32,
) -> Result<String, Box<dyn std::error::Error>> {
    prompt_template(
        "research_instructions",
        &[
            ("question", question.to_string()),
            ("today", current_date_label()),
            ("report_path", report_path.to_string()),
            ("min_calls", min_calls.to_string()),
        ],
    )
}

fn final_output(events: &[RuntimeEvent]) -> Option<Value> {
    events.iter().rev().find_map(|event| match event {
        RuntimeEvent::RunCompleted { output, .. } => Some(output.clone()),
        _ => None,
    })
}

fn print_trace(events: &[RuntimeEvent]) {
    for event in events {
        match event {
            RuntimeEvent::ToolCallStarted { tool, input, .. } => {
                println!("[tool] {tool} input={input}");
            }
            RuntimeEvent::ToolCallCompleted { tool, .. } => {
                println!("[tool:done] {tool}");
            }
            RuntimeEvent::SubAgentStarted { config_summary, .. } => {
                println!("[sub-agent] started {config_summary}");
            }
            RuntimeEvent::SubAgentCompleted { child_run_id, .. } => {
                println!("[sub-agent] completed child={child_run_id}");
            }
            RuntimeEvent::ChildRunEvent { event, .. } => {
                if let RuntimeEvent::ToolCallStarted { tool, input, .. } = event.as_ref() {
                    println!("[sub-agent:tool] {tool} input={input}");
                }
            }
            RuntimeEvent::RunCompleted { output, .. } => {
                println!("\n[final]\n{output}");
            }
            RuntimeEvent::RunFailed { error, .. } => {
                println!("\n[error] {error}");
            }
            _ => {}
        }
    }
}

fn parse_args() -> (String, String, u32) {
    let mut question = "How should Orchest expose sub-agent-as-tool ergonomics?".to_string();
    let mut report_path = DEFAULT_REPORT_PATH.to_string();
    let mut min_calls = env::var("DEEP_RESEARCH_MIN_CALLS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_MIN_RESEARCH_CALLS);
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--report" if index + 1 < args.len() => {
                report_path = args[index + 1].clone();
                index += 2;
            }
            "--min-research-calls" if index + 1 < args.len() => {
                if let Ok(value) = args[index + 1].parse::<u32>() {
                    min_calls = value;
                }
                index += 2;
            }
            _ => {
                question = args[index..].join(" ");
                break;
            }
        }
    }
    (question, report_path, min_calls)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    require_env("EXA_API_KEY")?;
    let (question, report_path, min_calls) = parse_args();
    if let Some(parent) = Path::new(&report_path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let web_model_name = require_env("WEB_SEARCH_MODEL")?;
    let deep_model_name = require_env("DEEP_RESEARCH_MODEL")?;
    let (web_config, web_model, web_registry) = build_web_search_agent()?;
    let (deep_config, deep_model, deep_registry) =
        build_deep_research_agent(web_config, web_model, web_registry, &report_path, min_calls)?;

    println!("[main:model] {deep_model_name}");
    println!("[web:model] {web_model_name}");
    println!("[min-research-calls] {min_calls}");
    println!("[question] {question}");

    let input = research_instructions(&question, &report_path, min_calls)?;
    let (handle, mut event_rx) = AgentRun::start(deep_config, input, deep_model, deep_registry);
    let mut events = Vec::new();
    while let Some(event) = event_rx.recv().await {
        events.push(event);
    }
    handle.wait().await;

    print_trace(&events);
    println!("\n[report] {report_path}");
    println!(
        "[raw-output] {}",
        final_output(&events).unwrap_or(Value::Null)
    );
    Ok(())
}
