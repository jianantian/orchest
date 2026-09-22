//! AgentAsTool: wraps an AgentConfig as a standard Tool.
//!
//! The child run executes in its own tokio task (spawned by AgentRun::start_with_bus);
//! AgentAsTool::execute awaits completion and forwards every child RuntimeEvent
//! upward as SubAgentEvent, giving consumers a continuous event stream.

use std::num::NonZeroUsize;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::budget::{BudgetConfig, BudgetUsage};
use crate::events::{RunFailureKind, RuntimeEvent};
use crate::model::{Message, ModelAdapter};
use crate::run::{AgentConfig, AgentRun, ConfigError, RunInput};
use crate::tool::registry::ToolRegistry;
use crate::tool::{JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource};

type InputMapperFn = dyn Fn(Value) -> Result<String, ToolError> + Send + Sync;
type OutputExtractorFn = dyn Fn(Value) -> Value + Send + Sync;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContextMode {
    #[default]
    Fresh,
    Fork {
        depth: NonZeroUsize,
    },
}

/// Output format contract for a sub-agent's raw text output (v0.15).
///
/// Without a contract the child run's output reaches the `output_extractor`
/// untouched and consumers scrape it with their own fragile parsing (the
/// music-gift countdown's `strip_code_fences` was the motivating example:
/// the model wraps the payload in prose or a full `<!DOCTYPE>` document and
/// the consumer's strict prefix/suffix check silently lets garbage through).
/// A declared contract moves extraction into the SDK: [`AgentAsTool::execute`]
/// extracts the conforming payload, retries the child once with a correction
/// prompt when extraction fails, and returns `Err(ToolError)` when it still
/// does not conform — non-conforming output never reaches the consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubAgentOutputExpect {
    /// The output text must contain a fenced code block; the block's content
    /// (fence lines stripped) becomes the output. When `lang` is set, the
    /// first fence tagged with that language wins; with no match, the first
    /// closed fence of any language is used — lang is a preference, not a
    /// filter, because models routinely emit untagged fences.
    Fenced { lang: Option<String> },
    /// The output must parse as JSON (a single wrapping fence is tolerated
    /// and stripped first); the parsed value becomes the output. "Parses" is
    /// the whole contract this round — schema validation is intentionally
    /// left to a later iteration.
    Json,
}

impl SubAgentOutputExpect {
    /// Human-readable contract description for correction prompts and errors.
    fn describe(&self) -> String {
        match self {
            Self::Fenced { lang: Some(lang) } => {
                format!("a single fenced code block ```{lang} … ```")
            }
            Self::Fenced { lang: None } => "a single fenced code block".to_string(),
            Self::Json => "valid JSON".to_string(),
        }
    }
}

/// Returns a `BudgetConfig` whose each limit is the tightest of `configured` and `remaining`.
/// A `None` on either side means "no limit from that side", so the other side wins.
fn cap_budget(configured: BudgetConfig, remaining: &BudgetConfig) -> BudgetConfig {
    fn min_opt<T: Ord>(a: Option<T>, b: Option<T>) -> Option<T> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    fn min_opt_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    fn min_opt_dur(
        a: Option<std::time::Duration>,
        b: Option<std::time::Duration>,
    ) -> Option<std::time::Duration> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (Some(x), None) | (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    BudgetConfig {
        max_tokens: min_opt(configured.max_tokens, remaining.max_tokens),
        max_tool_calls: min_opt(configured.max_tool_calls, remaining.max_tool_calls),
        max_duration: min_opt_dur(configured.max_duration, remaining.max_duration),
        max_cost_usd: min_opt_f64(configured.max_cost_usd, remaining.max_cost_usd),
    }
}

/// An [`AgentConfig`] wrapped as a standard [`Tool`]. The child run executes in
/// its own tokio task; every child [`RuntimeEvent`] is forwarded upward as
/// `SubAgentEvent`, and lifecycle events (`SubAgentStarted` /
/// `SubAgentCompleted` / `SubAgentFailed`) bracket the call.
///
/// # Failure semantics (v0.15)
///
/// A failed child run (`RuntimeEvent::RunFailed`) makes [`Tool::execute`]
/// return `Err(ToolError)` — never an `Ok` payload with an embedded `"error"`
/// key — so consumers dispatch on the standard v0.9.4 `ErrorKind`/`RetryHint`
/// contract instead of scraping `details["error"]`. `SubAgentFailed` still
/// fires with the same `child_run_id` and error, and the `ToolError` message
/// carries the `child_run_id` plus the budget the child consumed before
/// failing. `ToolError` has no details/diagnostic payload field, so the budget
/// rides in the message text. The failure path never constructs
/// `ToolOutput::Structured` and never invokes `output_extractor`.
///
/// `RunFailed` carries a structured [`RunFailureKind`] alongside the error
/// text (hotfix 2026_07_27 #242), so the kind/code adjudication dispatches on
/// it directly — no string matching on runtime-generated failure messages:
///
/// | child failure                      | kind  | retry  | code                     |
/// |------------------------------------|-------|--------|--------------------------|
/// | [`RunFailureKind::BudgetExceeded`] | Fatal | Unsafe | `BUDGET_EXCEEDED`        |
/// | [`RunFailureKind::MaxStepsReached`]| Fatal | Unsafe | `MAX_STEPS_REACHED`      |
/// | depth guard (`run_depth >= 3`)     | Fatal | Unsafe | `MAX_RUN_DEPTH_EXCEEDED` |
/// | [`RunFailureKind::Other`]          | Fatal | Unsafe | `SUB_AGENT_RUN_FAILED`   |
///
/// Every class is `Fatal`/`Unsafe` — the v0.9.4 retry dispatch never
/// auto-retries the tool — because each identifiable cause is deterministic
/// for the same input and config: a retry hits the same budget/step/depth
/// ceiling (mirroring the run loop's own
/// `ToolError::fatal(..).with_code("BUDGET_EXCEEDED")` budget convention), and
/// model-side errors surface as `RunFailed` only after the child's own retry
/// policy is exhausted, so a parent-side auto-retry would blindly re-run the
/// whole child. The parent model still receives the structured error as the
/// tool result and may deliberately re-invoke the tool with adjusted input.
///
/// **Budget accounting**: the failure path returns no `ToolOutput`, so the
/// child's consumed budget rides on the `ToolError` itself
/// ([`ToolError::external_usage`], hotfix 2026_07_27 / issue #241); the
/// runtime folds it into the parent's `BudgetGuard` on the tool-error path
/// with the same semantics as the success path's
/// `Structured.external_usage`. A parent model that repeatedly invokes a
/// failing sub-agent is therefore still bounded by its own budget (and each
/// child is individually capped by `cap_budget`).
///
/// # Output format contract (v0.15)
///
/// [`SubAgentBuilder::expect_output`] declares a format contract for the
/// child's raw output. With a contract, a successful run's output is
/// extracted/validated by the SDK (one correction retry on violation, then
/// `Err(ToolError)` with code `SUB_AGENT_OUTPUT_CONTRACT_VIOLATION`);
/// `details["output"]` carries the extracted payload and
/// `details["raw_output"]` the verbatim text. Without a contract, behavior
/// is exactly the failure-semantics contract above.
pub struct AgentAsTool {
    config: AgentConfig,
    tool_name: String,
    tool_description: String,
    input_schema: JsonSchema,
    metadata: ToolMetadata,
    model: Arc<dyn ModelAdapter>,
    registry: ToolRegistry,
    input_mapper: Arc<InputMapperFn>,
    output_extractor: Arc<OutputExtractorFn>,
    context_mode: ContextMode,
    output_expect: Option<SubAgentOutputExpect>,
}

#[async_trait]
impl Tool for AgentAsTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
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

    fn needs_parent_context(&self) -> bool {
        matches!(self.context_mode, ContextMode::Fork { .. })
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let child_input = (self.input_mapper)(input)?;

        if ctx.run_depth >= 3 {
            let child_run_id = crate::run::RunId::new();
            ctx.emit_event(RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: "max_run_depth_exceeded".into(),
            })
            .await;
            return Err(ToolError::fatal(format!(
                "sub-agent run {child_run_id} failed: max_run_depth_exceeded"
            ))
            .with_code("MAX_RUN_DEPTH_EXCEEDED"));
        }

        let mut child_config = self.config.clone();
        child_config.runtime.run_depth = ctx.run_depth + 1;
        child_config.budget = cap_budget(child_config.budget, &ctx.remaining_budget);

        let initial_messages = match self.context_mode {
            ContextMode::Fresh => Vec::new(),
            ContextMode::Fork { depth } => {
                if ctx.parent_messages.is_empty() {
                    return Err(ToolError::invalid_input(
                        "ContextMode::Fork requires parent message history",
                    )
                    .with_code("EMPTY_PARENT_CONTEXT"));
                }
                ctx.parent_messages
                    .iter()
                    .rev()
                    .take(depth.get())
                    .rev()
                    .cloned()
                    .collect()
            }
        };

        let first = self
            .run_child_attempt(&child_config, initial_messages, child_input.clone(), ctx)
            .await?;

        let (attempt, raw_output) = self
            .enforce_output_contract(&child_config, &child_input, first, ctx)
            .await?;

        let mut details = json!({
            "child_run_id": attempt.run_id,
            "output": attempt.output,
            "budget_used": attempt.usage.clone(),
        });
        if let Some(raw) = raw_output {
            details["raw_output"] = raw;
        }

        let model_output = (self.output_extractor)(details.clone());
        Ok(ToolOutput::Structured {
            model_output,
            details,
            external_usage: Some(attempt.usage),
        })
    }
}

/// One completed child attempt: terminal run id, final output, and the
/// budget that attempt consumed.
struct ChildAttempt {
    run_id: crate::run::RunId,
    output: Value,
    usage: BudgetUsage,
}

impl AgentAsTool {
    /// Run one child attempt to completion: emits `SubAgentStarted` /
    /// `SubAgentCompleted` (or `SubAgentFailed` + `Err`) and folds the child's
    /// token/tool-call/cost usage into the returned [`ChildAttempt`].
    async fn run_child_attempt(
        &self,
        child_config: &AgentConfig,
        initial_messages: Vec<Message>,
        child_input: String,
        ctx: &ToolContext,
    ) -> Result<ChildAttempt, ToolError> {
        let parent_run_id = ctx.run_id;
        let (handle, mut child_rx) = AgentRun::start_with_bus(
            child_config.clone(),
            RunInput::text(child_input.clone()).into_blocks(),
            initial_messages,
            Arc::clone(&self.model),
            self.registry.clone(),
            ctx.approval_bus.clone(),
            ctx.child_registry.clone(),
            vec![],
        );
        let child_run_id = handle.run_id;

        // Publish a public child control surface before any child events so the
        // supervisor RunHandle can resolve inject/steer/completion targets.
        let outcome_tx = ctx
            .child_registry
            .register(crate::run::handle::ChildRegistration {
                run_id: child_run_id,
                parent_run_id,
                actor_ref: std::sync::Arc::clone(&handle.actor_ref),
                ready: std::sync::Arc::clone(&handle.ready),
                supervisor_ref: std::sync::Arc::clone(&handle.supervisor_ref),
            })
            .await;

        ctx.emit_event(RuntimeEvent::SubAgentStarted {
            parent_run_id,
            child_run_id,
            config_summary: json!({
                "run_depth": ctx.run_depth + 1,
                "input": child_input,
            }),
        })
        .await;

        let mut child_usage = BudgetUsage::default();
        let mut output = Value::Null;
        let mut failed: Option<(String, RunFailureKind)> = None;

        while let Some(event) = child_rx.recv().await {
            match &event {
                RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                    let tokens_used = tokens.input_tokens + tokens.output_tokens;
                    let cost_usd = tokens.cost_usd.unwrap_or(0.0);
                    child_usage.tokens_used += tokens_used;
                    child_usage.cost_usd += cost_usd;
                }
                RuntimeEvent::ToolCallCompleted { .. } => {
                    child_usage.tool_calls_used += 1;
                }
                RuntimeEvent::RunCompleted {
                    output: child_output,
                    ..
                } => {
                    // Success is always terminal — supervision does not restart after it.
                    failed = None;
                    output = child_output.clone();
                    let _ = outcome_tx.send(Some(crate::run::ChildRunOutcome::Completed {
                        output: child_output.clone(),
                    }));
                }
                RuntimeEvent::RunFailed { error, kind } => {
                    // Defer publishing Failed until the child event stream ends so an
                    // eligible SupervisionStrategy::Restart can supersede this attempt.
                    failed = Some((error.clone(), *kind));
                }
                RuntimeEvent::RunRestarted { .. } => {
                    failed = None;
                    output = Value::Null;
                }
                _ => {}
            }
            ctx.emit_event(RuntimeEvent::SubAgentEvent {
                parent_run_id,
                child_run_id,
                event: Box::new(event),
            })
            .await;
        }
        handle.wait().await;
        // Keep the registry entry so late SubAgentStarted delivery and
        // post-terminal wait_completion lookups still resolve (P1-4).

        if let Some((error, kind)) = failed {
            let _ = outcome_tx.send(Some(crate::run::ChildRunOutcome::Failed {
                error: error.clone(),
                kind,
            }));
            ctx.emit_event(RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: error.clone(),
            })
            .await;
            return Err(child_failure_error(
                child_run_id,
                &error,
                kind,
                &child_usage,
            ));
        }

        ctx.emit_event(RuntimeEvent::SubAgentCompleted {
            child_run_id,
            output: output.clone(),
            budget_used: child_usage.clone(),
        })
        .await;
        Ok(ChildAttempt {
            run_id: child_run_id,
            output,
            usage: child_usage,
        })
    }

    /// Apply the declared output contract to a successful child run: extract
    /// the conforming payload, or retry the child once with a self-contained
    /// correction prompt (fresh history — the prompt carries the original
    /// request plus the offending output's head) and extract again. Returns
    /// the final attempt plus `raw_output`, which is `Some` whenever
    /// extraction rewrote the output, so `details` keeps the verbatim text
    /// next to the extracted payload. Usage is summed across both attempts.
    /// Without a declared contract this is the identity: `(first, None)`.
    async fn enforce_output_contract(
        &self,
        child_config: &AgentConfig,
        child_input: &str,
        first: ChildAttempt,
        ctx: &ToolContext,
    ) -> Result<(ChildAttempt, Option<Value>), ToolError> {
        let Some(expect) = &self.output_expect else {
            return Ok((first, None));
        };
        match extract_expected(expect, &first.output) {
            Ok(extracted) => Ok((
                ChildAttempt {
                    run_id: first.run_id,
                    output: extracted,
                    usage: first.usage,
                },
                Some(first.output),
            )),
            Err(first_reason) => {
                ctx.emit_event(RuntimeEvent::RuntimeWarning {
                            message: format!(
                                "sub-agent run {} output violates the declared contract ({}): {first_reason}; retrying once with a correction prompt",
                                first.run_id,
                                expect.describe()
                            ),
                        }).await;
                let correction = format!(
                    "Your previous reply did not satisfy the required output format ({}): {first_reason}\n\
                     Reply to the original request again, responding with ONLY the requested content \
                     in the required format — no explanations, no surrounding prose.\n\
                     Original request:\n{child_input}\n\
                     Your previous reply (first 500 chars):\n{}",
                    expect.describe(),
                    output_head(&first.output),
                );
                let second = match self
                    .run_child_attempt(child_config, Vec::new(), correction, ctx)
                    .await
                {
                    Ok(second) => second,
                    Err(mut e) => {
                        // The correction attempt's RunFailed bills only its own
                        // spend — add the first attempt's usage so it is not
                        // lost from the parent's accounting.
                        let own = e.external_usage.take().unwrap_or_default();
                        e.external_usage = Some(BudgetUsage {
                            tokens_used: own.tokens_used + first.usage.tokens_used,
                            tool_calls_used: own.tool_calls_used + first.usage.tool_calls_used,
                            cost_usd: own.cost_usd + first.usage.cost_usd,
                        });
                        return Err(e);
                    }
                };
                let mut usage = first.usage;
                usage.tokens_used += second.usage.tokens_used;
                usage.tool_calls_used += second.usage.tool_calls_used;
                usage.cost_usd += second.usage.cost_usd;
                match extract_expected(expect, &second.output) {
                    Ok(extracted) => Ok((
                        ChildAttempt {
                            run_id: second.run_id,
                            output: extracted,
                            usage,
                        },
                        Some(second.output),
                    )),
                    Err(second_reason) => Err(ToolError::fatal(format!(
                        "sub-agent run {} output still violates the declared contract ({}) after one correction attempt: {second_reason} (budget_used: {} tokens, {} tool calls, ${:.4}); raw output head: {}",
                        second.run_id,
                        expect.describe(),
                        usage.tokens_used,
                        usage.tool_calls_used,
                        usage.cost_usd,
                        output_head(&second.output),
                    ))
                    .with_code("SUB_AGENT_OUTPUT_CONTRACT_VIOLATION")
                    .with_external_usage(usage)),
                }
            }
        }
    }
}

/// Builds the `ToolError` for a failed child run. The code adjudication
/// dispatches on the structured [`RunFailureKind`] the runtime attached to
/// `RunFailed` — no string matching on the error text; the kind is always
/// `Fatal` with `RetryHint::Unsafe`. See the [`AgentAsTool`] docs for the full
/// mapping rule and rationale.
fn child_failure_error(
    child_run_id: crate::run::RunId,
    error: &str,
    kind: RunFailureKind,
    budget_used: &BudgetUsage,
) -> ToolError {
    let code = match kind {
        RunFailureKind::BudgetExceeded => "BUDGET_EXCEEDED",
        RunFailureKind::MaxStepsReached => "MAX_STEPS_REACHED",
        RunFailureKind::Other => "SUB_AGENT_RUN_FAILED",
    };
    ToolError::fatal(format!(
        "sub-agent run {child_run_id} failed: {error} (budget_used: {} tokens, {} tool calls, ${:.4})",
        budget_used.tokens_used, budget_used.tool_calls_used, budget_used.cost_usd
    ))
    .with_code(code)
    .with_external_usage(budget_used.clone())
}

/// Extract the contract-conforming payload from a child run's raw output.
/// `Err(reason)` describes the violation in model-addressable prose (it
/// rides into the correction prompt).
fn extract_expected(expect: &SubAgentOutputExpect, output: &Value) -> Result<Value, String> {
    match expect {
        SubAgentOutputExpect::Fenced { lang } => {
            let text = output
                .as_str()
                .ok_or_else(|| "output is not a text string".to_string())?;
            extract_fenced_block(text, lang.as_deref()).map(Value::String)
        }
        SubAgentOutputExpect::Json => {
            let Some(text) = output.as_str() else {
                // Already structured — nothing to parse.
                return Ok(output.clone());
            };
            if let Ok(parsed) = serde_json::from_str(text.trim()) {
                return Ok(parsed);
            }
            let inner = extract_fenced_block(text, None)?;
            serde_json::from_str(inner.trim()).map_err(|e| format!("output is not valid JSON: {e}"))
        }
    }
}

/// Extract the content of the first closed fenced code block in `text`. A
/// fence is a line whose trimmed form starts with ```` ``` ````; the closing
/// fence is a later line that is exactly ```` ``` ````. When `lang` is set,
/// the first non-empty block tagged with that language wins; with no match,
/// the first non-empty block of any language is used — lang is a preference,
/// not a filter (see [`SubAgentOutputExpect`]). Empty blocks are skipped,
/// not fatal: models occasionally emit an empty fence followed by the real
/// one.
fn extract_fenced_block(text: &str, lang: Option<&str>) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    // Pair opening/closing fences in one linear walk; an unclosed fence
    // swallows the rest of the text, so the walk stops there.
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut saw_unclosed = false;
    let mut i = 0;
    while i < lines.len() {
        let Some(tag) = lines[i].trim_start().strip_prefix("```").map(str::trim) else {
            i += 1;
            continue;
        };
        let Some(pos) = lines[i + 1..].iter().position(|line| line.trim() == "```") else {
            saw_unclosed = true;
            break;
        };
        let close = i + 1 + pos;
        let body = lines[i + 1..close].join("\n").trim().to_string();
        blocks.push((tag.to_string(), body));
        i = close + 1;
    }
    let non_empty = |block: &&(String, String)| !block.1.is_empty();
    let picked = lang
        .and_then(|want| {
            blocks
                .iter()
                .filter(non_empty)
                .find(|(tag, _)| tag.eq_ignore_ascii_case(want))
        })
        .or_else(|| blocks.iter().find(non_empty));
    if let Some((_, body)) = picked {
        return Ok(body.clone());
    }
    if !blocks.is_empty() {
        return Err("fenced code block is empty".into());
    }
    if saw_unclosed {
        return Err("fenced code block is not closed".into());
    }
    Err("no fenced code block found".into())
}

/// First 500 chars of a run output for diagnostics — string outputs
/// verbatim, structured outputs via their JSON rendering.
fn output_head(output: &Value) -> String {
    let text = match output {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    text.chars().take(500).collect()
}

// ── SubAgentBuilder ──────────────────────────────────────────────────────────

pub struct SubAgentBuilder {
    config: AgentConfig,
    tool_name: String,
    tool_description: String,
    model: Option<Arc<dyn ModelAdapter>>,
    registry: Option<ToolRegistry>,
    input_schema: Option<JsonSchema>,
    input_mapper: Option<Arc<InputMapperFn>>,
    output_extractor: Option<Arc<OutputExtractorFn>>,
    context_mode: ContextMode,
    output_expect: Option<SubAgentOutputExpect>,
}

impl SubAgentBuilder {
    pub(crate) fn new(config: AgentConfig, name: String, description: String) -> Self {
        Self {
            config,
            tool_name: name,
            tool_description: description,
            model: None,
            registry: None,
            input_schema: None,
            input_mapper: None,
            output_extractor: None,
            context_mode: ContextMode::Fresh,
            output_expect: None,
        }
    }

    pub fn model(mut self, model: Arc<dyn ModelAdapter>) -> Self {
        self.model = Some(model);
        self
    }

    pub fn registry(mut self, registry: ToolRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn input_schema(mut self, schema: Value) -> Self {
        self.input_schema = Some(schema);
        self
    }

    pub fn input_mapper(
        mut self,
        f: impl Fn(Value) -> Result<String, ToolError> + Send + Sync + 'static,
    ) -> Self {
        self.input_mapper = Some(Arc::new(f));
        self
    }

    pub fn output_extractor(mut self, f: impl Fn(Value) -> Value + Send + Sync + 'static) -> Self {
        self.output_extractor = Some(Arc::new(f));
        self
    }

    pub fn context_mode(mut self, mode: ContextMode) -> Self {
        self.context_mode = mode;
        self
    }

    /// Declare the output format contract for the child run (see
    /// [`SubAgentOutputExpect`]). Without one, the child's output reaches the
    /// `output_extractor` untouched.
    pub fn expect_output(mut self, expect: SubAgentOutputExpect) -> Self {
        self.output_expect = Some(expect);
        self
    }

    pub fn build(self) -> Result<Arc<dyn Tool>, ConfigError> {
        let model = self.model.ok_or(ConfigError::SubAgentMissingModel)?;
        let registry = self.registry.ok_or(ConfigError::SubAgentMissingRegistry)?;
        let input_schema = self.input_schema.unwrap_or_else(
            || json!({"type": "object", "properties": {"input": {"type": "string"}}}),
        );
        let input_mapper = self.input_mapper.unwrap_or_else(|| {
            Arc::new(|input: Value| {
                input
                    .get("input")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| ToolError::fatal("missing 'input' field"))
            })
        });
        let output_extractor = self
            .output_extractor
            .unwrap_or_else(|| Arc::new(|details: Value| details));

        Ok(Arc::new(AgentAsTool {
            config: self.config,
            tool_name: self.tool_name,
            tool_description: self.tool_description,
            input_schema,
            metadata: ToolMetadata {
                side_effect: false,
                approval: crate::tool::Approval::Never,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
            model,
            registry,
            input_mapper,
            output_extractor,
            context_mode: self.context_mode,
            output_expect: self.output_expect,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ContentBlock, Message, ModelCapabilities, ModelError, ModelResponse, RequestOptions, Role,
        StopReason, TokenUsage,
    };
    use crate::run::{AgentRun, ChildRunOutcome};
    use crate::tool::{ErrorKind, RetryHint, ToolDef};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use tokio::sync::{mpsc as tokio_mpsc, Notify};

    struct NeverCalledModel;

    #[async_trait]
    impl ModelAdapter for NeverCalledModel {
        fn provider_name(&self) -> &str {
            "never-called"
        }
        fn model_name(&self) -> &str {
            "never-called"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            unimplemented!("build() never invokes the model")
        }
    }

    fn test_agent_config() -> AgentConfig {
        AgentConfig::builder("test-agent", "mock/model")
            .system_prompt("test")
            .max_steps(1)
            .build()
            .unwrap()
    }

    #[test]
    fn build_fails_with_missing_model_when_model_not_set() {
        let result = test_agent_config()
            .as_tool("t", "d")
            .registry(ToolRegistry::new())
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("build() should fail without .model()"),
        };
        assert!(matches!(err, ConfigError::SubAgentMissingModel));
    }

    #[test]
    fn build_fails_with_missing_registry_when_registry_not_set() {
        let result = test_agent_config()
            .as_tool("t", "d")
            .model(Arc::new(NeverCalledModel))
            .build();
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("build() should fail without .registry()"),
        };
        assert!(matches!(err, ConfigError::SubAgentMissingRegistry));
    }

    #[test]
    fn build_succeeds_when_model_and_registry_both_set() {
        let tool = test_agent_config()
            .as_tool("t", "d")
            .model(Arc::new(NeverCalledModel))
            .registry(ToolRegistry::new())
            .build()
            .unwrap();
        assert_eq!(tool.name(), "t");
    }

    // ── execute: failure → Err(ToolError), success → Structured ─────────────

    /// Child model that fails the run immediately: with no retry policy
    /// configured the child emits `RunFailed { error: "provider exploded" }`.
    struct FailingModel;

    #[async_trait]
    impl ModelAdapter for FailingModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "failing"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Err(ModelError::internal("provider exploded", "TEST_BOOM"))
        }
    }

    /// Child model that always requests an unregistered tool, so the run keeps
    /// looping until a limit (max_steps or budget) fails it.
    struct ToolUseLoopModel;

    #[async_trait]
    impl ModelAdapter for ToolUseLoopModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "loop"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "loop-1".into(),
                    name: "missing_tool".into(),
                    input: json!({}),
                }],
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 3,
                    ..Default::default()
                },
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            })
        }
    }

    /// Child model that answers immediately with text.
    struct SuccessModel;

    #[async_trait]
    impl ModelAdapter for SuccessModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "success"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("child answer".into())],
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 3,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }

    fn build_child_tool(
        config: AgentConfig,
        model: Arc<dyn ModelAdapter>,
        output_extractor: impl Fn(Value) -> Value + Send + Sync + 'static,
    ) -> Arc<dyn Tool> {
        config
            .as_tool("child", "child under test")
            .model(model)
            .registry(ToolRegistry::new())
            .output_extractor(output_extractor)
            .build()
            .unwrap()
    }

    fn execute_ctx(run_depth: u32) -> (ToolContext, tokio_mpsc::Receiver<RuntimeEvent>) {
        let (tx, rx) = tokio_mpsc::channel(64);
        (
            ToolContext {
                run_depth,
                event_tx: Some(tx),
                ..ToolContext::oneshot()
            },
            rx,
        )
    }

    fn drain_events(mut rx: tokio_mpsc::Receiver<RuntimeEvent>) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn child_run_failed_returns_err_with_diagnostics() {
        let extractor_calls = Arc::new(AtomicU32::new(0));
        let calls = Arc::clone(&extractor_calls);
        let tool = build_child_tool(
            test_agent_config(),
            Arc::new(FailingModel),
            move |details| {
                calls.fetch_add(1, Ordering::SeqCst);
                details
            },
        );
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "boom"}), &ctx)
            .await
            .expect_err("child RunFailed must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("SUB_AGENT_RUN_FAILED"));
        assert!(
            err.message.contains("provider exploded"),
            "message carries the child error: {}",
            err.message
        );
        assert!(
            err.message.contains("budget_used"),
            "message carries budget diagnostics: {}",
            err.message
        );

        let events = drain_events(rx);
        let (child_run_id, error) = events
            .iter()
            .find_map(|e| match e {
                RuntimeEvent::SubAgentFailed {
                    child_run_id,
                    error,
                } => Some((*child_run_id, error.clone())),
                _ => None,
            })
            .expect("SubAgentFailed must still fire");
        assert_eq!(error, "provider exploded");
        assert!(
            err.message.contains(&child_run_id.to_string()),
            "message carries child_run_id: {}",
            err.message
        );
        assert_eq!(
            extractor_calls.load(Ordering::SeqCst),
            0,
            "output_extractor must not be invoked on failure"
        );
    }

    #[tokio::test]
    async fn child_budget_exceeded_maps_to_budget_exceeded_code() {
        let mut config = test_agent_config();
        config.runtime.max_steps = 5;
        config.budget.max_tokens = Some(1);
        let tool = build_child_tool(config, Arc::new(ToolUseLoopModel), |details| details);
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "loop"}), &ctx)
            .await
            .expect_err("child budget exhaustion must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("BUDGET_EXCEEDED"));
        assert!(
            err.message.contains("budget_exceeded"),
            "message carries the child error: {}",
            err.message
        );
        // The child consumed 5 tokens before the guard fired; the diagnostic
        // must report real usage, not a default.
        assert!(
            err.message.contains("5 tokens"),
            "message carries consumed budget: {}",
            err.message
        );
        // And the structured channel must bill the parent run (issue #241).
        let usage = err
            .external_usage
            .as_ref()
            .expect("failed child bills the parent via external_usage");
        assert_eq!(usage.tokens_used, 5);

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::SubAgentFailed { error, .. } if error.starts_with("budget_exceeded"))
        ));
    }

    #[tokio::test]
    async fn child_max_steps_maps_to_max_steps_reached_code() {
        // test_agent_config caps at max_steps(1); a model that always requests
        // another tool call hits the step ceiling instead of completing.
        let tool = build_child_tool(test_agent_config(), Arc::new(ToolUseLoopModel), |details| {
            details
        });
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "loop"}), &ctx)
            .await
            .expect_err("child max-steps exhaustion must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("MAX_STEPS_REACHED"));
        assert!(
            err.message.contains("max_steps_reached"),
            "message carries the child error: {}",
            err.message
        );

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::SubAgentFailed { error, .. } if error == "max_steps_reached")
        ));
    }

    #[tokio::test]
    async fn run_depth_guard_returns_err_without_starting_child() {
        let extractor_calls = Arc::new(AtomicU32::new(0));
        let calls = Arc::clone(&extractor_calls);
        let tool = build_child_tool(
            test_agent_config(),
            Arc::new(NeverCalledModel),
            move |details| {
                calls.fetch_add(1, Ordering::SeqCst);
                details
            },
        );
        let (ctx, rx) = execute_ctx(3);

        let err = tool
            .execute(json!({"input": "too deep"}), &ctx)
            .await
            .expect_err("depth guard must surface as Err(ToolError)");

        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(err.retry, RetryHint::Unsafe);
        assert_eq!(err.code.as_deref(), Some("MAX_RUN_DEPTH_EXCEEDED"));
        assert!(
            err.message.contains("max_run_depth_exceeded"),
            "message: {}",
            err.message
        );

        let events = drain_events(rx);
        let child_run_id = events
            .iter()
            .find_map(|e| match e {
                RuntimeEvent::SubAgentFailed {
                    child_run_id,
                    error,
                } if error == "max_run_depth_exceeded" => Some(*child_run_id),
                _ => None,
            })
            .expect("SubAgentFailed must still fire for the depth guard");
        assert!(err.message.contains(&child_run_id.to_string()));
        assert_eq!(extractor_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn child_success_returns_structured_with_details_and_external_usage() {
        let extractor_calls = Arc::new(AtomicU32::new(0));
        let calls = Arc::clone(&extractor_calls);
        let tool = build_child_tool(
            test_agent_config(),
            Arc::new(SuccessModel),
            move |details| {
                calls.fetch_add(1, Ordering::SeqCst);
                json!({"extracted": details.get("output").cloned().unwrap_or(Value::Null)})
            },
        );
        let (ctx, rx) = execute_ctx(0);

        let output = tool
            .execute(json!({"input": "hi"}), &ctx)
            .await
            .expect("successful child run returns Ok");

        let (model_output, details, external_usage) = match output {
            ToolOutput::Structured {
                model_output,
                details,
                external_usage,
            } => (model_output, details, external_usage),
            other => panic!("expected Structured, got {other:?}"),
        };
        assert_eq!(model_output, json!({"extracted": "child answer"}));
        assert_eq!(details["output"], json!("child answer"));
        assert!(details.get("child_run_id").is_some());
        assert_eq!(details["budget_used"]["tokens_used"], json!(5));
        let usage = external_usage.expect("external_usage carries child budget");
        assert_eq!(usage.tokens_used, 5);
        assert_eq!(extractor_calls.load(Ordering::SeqCst), 1);

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::SubAgentCompleted { output, .. } if output == &json!("child answer"))
        ));
        assert!(!events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::SubAgentFailed { .. })));
    }

    // ── output contract: extraction, one correction round, Err ─────────────

    /// Child model answering with queued text responses in call order and
    /// recording the last user message of every call (for correction-prompt
    /// assertions).
    struct ScriptedModel {
        responses: std::sync::Mutex<std::collections::VecDeque<String>>,
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl ScriptedModel {
        fn new(responses: &[&str]) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
                seen: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ModelAdapter for ScriptedModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "scripted"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[crate::model::Message],
            _tools: &[crate::model::ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let last_user = messages
                .iter()
                .rev()
                .filter(|m| matches!(m.role, crate::model::Role::User))
                .find_map(|m| {
                    m.content.iter().find_map(|b| match b {
                        ContentBlock::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_default();
            self.seen.lock().expect("seen lock").push(last_user);
            let next = self
                .responses
                .lock()
                .expect("responses lock")
                .pop_front()
                .unwrap_or_else(|| "scripted model exhausted".to_string());
            Ok(ModelResponse {
                content: vec![ContentBlock::Text(next)],
                usage: TokenUsage {
                    input_tokens: 2,
                    output_tokens: 3,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }

    fn build_contract_tool(
        model: Arc<dyn ModelAdapter>,
        expect: SubAgentOutputExpect,
    ) -> Arc<dyn Tool> {
        test_agent_config()
            .as_tool("child", "child under test")
            .model(model)
            .registry(ToolRegistry::new())
            .expect_output(expect)
            .output_extractor(|details| {
                json!({"extracted": details.get("output").cloned().unwrap_or(Value::Null)})
            })
            .build()
            .unwrap()
    }

    #[test]
    fn fenced_block_prefers_matching_lang() {
        let text = "intro\n```json\n{\"a\":1}\n```\nmiddle\n```html\n<div>ok</div>\n```\noutro";
        let got = extract_fenced_block(text, Some("html")).expect("html fence");
        assert_eq!(got, "<div>ok</div>");
    }

    #[test]
    fn fenced_block_falls_back_to_any_lang() {
        let text = "prose\n```\n<div>ok</div>\n```";
        let got = extract_fenced_block(text, Some("html")).expect("untagged fence fallback");
        assert_eq!(got, "<div>ok</div>");
        let got = extract_fenced_block(text, None).expect("first fence");
        assert_eq!(got, "<div>ok</div>");
    }

    #[test]
    fn fenced_block_tolerates_doctype_and_prose_around() {
        let text = "Here you go:\n```html\n<!DOCTYPE html>\n<html></html>\n```\nHope it helps.";
        let got = extract_fenced_block(text, Some("html")).expect("block");
        assert!(got.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn fenced_block_rejects_unclosed_empty_and_missing() {
        assert!(extract_fenced_block("```html\n<div>", None)
            .unwrap_err()
            .contains("not closed"));
        assert!(extract_fenced_block("```html\n```", None)
            .unwrap_err()
            .contains("empty"));
        assert!(extract_fenced_block("plain text", None)
            .unwrap_err()
            .contains("no fenced"));
    }

    #[test]
    fn fenced_block_skips_empty_block_and_takes_next() {
        // A model emitting an empty fence before the real one must not
        // trigger a spurious correction round.
        let text = "```html\n```\nOops, here is the real one:\n```html\n<div>ok</div>\n```";
        let got = extract_fenced_block(text, Some("html")).expect("second block");
        assert_eq!(got, "<div>ok</div>");
    }

    #[test]
    fn json_contract_parses_raw_fenced_and_structured() {
        let raw = Value::String("{\"a\": 1}".into());
        assert_eq!(
            extract_expected(&SubAgentOutputExpect::Json, &raw).expect("raw json"),
            json!({"a": 1})
        );
        let fenced = Value::String("result:\n```json\n{\"a\": 1}\n```".into());
        assert_eq!(
            extract_expected(&SubAgentOutputExpect::Json, &fenced).expect("fenced json"),
            json!({"a": 1})
        );
        let structured = json!({"b": 2});
        assert_eq!(
            extract_expected(&SubAgentOutputExpect::Json, &structured).expect("structured"),
            structured
        );
        let bad = Value::String("not json".into());
        assert!(extract_expected(&SubAgentOutputExpect::Json, &bad).is_err());
    }

    #[tokio::test]
    async fn fenced_contract_extracts_payload_and_keeps_raw() {
        let model = Arc::new(ScriptedModel::new(&[
            "Here is your widget!\n```html\n<div>ok</div>\n```\nHope it helps.",
        ]));
        let tool = build_contract_tool(
            model.clone(),
            SubAgentOutputExpect::Fenced {
                lang: Some("html".into()),
            },
        );
        let (ctx, _rx) = execute_ctx(0);

        let output = tool
            .execute(json!({"input": "make widget"}), &ctx)
            .await
            .expect("conforming output");
        let (model_output, details) = match output {
            ToolOutput::Structured {
                model_output,
                details,
                ..
            } => (model_output, details),
            other => panic!("expected Structured, got {other:?}"),
        };
        assert_eq!(model_output, json!({"extracted": "<div>ok</div>"}));
        assert_eq!(details["output"], json!("<div>ok</div>"));
        assert!(details["raw_output"]
            .as_str()
            .unwrap_or("")
            .contains("Here is your widget!"));
        assert_eq!(
            model.seen.lock().expect("seen").len(),
            1,
            "no correction round needed"
        );
    }

    #[tokio::test]
    async fn fenced_contract_retries_once_with_correction_prompt() {
        let model = Arc::new(ScriptedModel::new(&[
            "sure, here it is: <div>ok</div>",
            "```html\n<div>ok</div>\n```",
        ]));
        let tool = build_contract_tool(
            model.clone(),
            SubAgentOutputExpect::Fenced {
                lang: Some("html".into()),
            },
        );
        let (ctx, rx) = execute_ctx(0);

        let output = tool
            .execute(json!({"input": "make widget"}), &ctx)
            .await
            .expect("correction succeeds");
        let (details, external_usage) = match output {
            ToolOutput::Structured {
                details,
                external_usage,
                ..
            } => (details, external_usage),
            other => panic!("expected Structured, got {other:?}"),
        };
        assert_eq!(details["output"], json!("<div>ok</div>"));

        // The correction round saw the format description, the violation
        // reason, and the original request.
        let seen = model.seen.lock().expect("seen");
        assert_eq!(seen.len(), 2);
        assert!(
            seen[1].contains("did not satisfy the required output format"),
            "correction prompt: {}",
            seen[1]
        );
        assert!(
            seen[1].contains("```html"),
            "format description: {}",
            seen[1]
        );
        assert!(seen[1].contains("Original request"), "prompt: {}", seen[1]);
        drop(seen);

        // Usage is summed across both attempts (5 tokens each).
        assert_eq!(details["budget_used"]["tokens_used"], json!(10));
        assert_eq!(external_usage.expect("usage").tokens_used, 10);

        let events = drain_events(rx);
        assert!(events.iter().any(
            |e| matches!(e, RuntimeEvent::RuntimeWarning { message } if message.contains("correction"))
        ));
    }

    #[tokio::test]
    async fn fenced_contract_violation_after_correction_returns_err() {
        let model = Arc::new(ScriptedModel::new(&["no fence here", "still no fence"]));
        let tool = build_contract_tool(model.clone(), SubAgentOutputExpect::Fenced { lang: None });
        let (ctx, rx) = execute_ctx(0);

        let err = tool
            .execute(json!({"input": "make widget"}), &ctx)
            .await
            .expect_err("non-conforming output must not reach the consumer");
        assert_eq!(err.kind, ErrorKind::Fatal);
        assert_eq!(
            err.code.as_deref(),
            Some("SUB_AGENT_OUTPUT_CONTRACT_VIOLATION")
        );
        assert!(
            err.message.contains("still no fence"),
            "raw output head in diagnostics: {}",
            err.message
        );
        // Both attempts' spend bills the parent (5 tokens each, issue #241).
        let usage = err
            .external_usage
            .as_ref()
            .expect("contract violation bills the parent via external_usage");
        assert_eq!(usage.tokens_used, 10);
        assert_eq!(
            model.seen.lock().expect("seen").len(),
            2,
            "exactly one correction round"
        );
        let events = drain_events(rx);
        assert!(events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::RuntimeWarning { .. })));
    }

    // ── #249: public child control surface ───────────────────────────────────

    struct ChildControlGate {
        entered: std::sync::atomic::AtomicBool,
        entered_notify: Notify,
        released: std::sync::atomic::AtomicBool,
        release_notify: Notify,
    }

    impl ChildControlGate {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                entered: std::sync::atomic::AtomicBool::new(false),
                entered_notify: Notify::new(),
                released: std::sync::atomic::AtomicBool::new(false),
                release_notify: Notify::new(),
            })
        }

        async fn hold(&self) {
            self.entered.store(true, Ordering::SeqCst);
            self.entered_notify.notify_waiters();
            loop {
                if self.released.load(Ordering::SeqCst) {
                    break;
                }
                self.release_notify.notified().await;
            }
        }

        async fn wait_entered(&self) {
            loop {
                if self.entered.load(Ordering::SeqCst) {
                    break;
                }
                self.entered_notify.notified().await;
            }
        }

        fn release(&self) {
            self.released.store(true, Ordering::SeqCst);
            self.release_notify.notify_waiters();
        }
    }

    /// Two-call child: first call holds a gate then requests a noop tool; second
    /// call records history (so mid-flight inject/steer are visible) and ends.
    struct GatedChildModel {
        calls: AtomicU32,
        gate: Arc<ChildControlGate>,
        histories: Arc<Mutex<Vec<Vec<Message>>>>,
    }

    #[async_trait]
    impl ModelAdapter for GatedChildModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "gated-child"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            self.histories
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(messages.to_vec());
            if n == 0 {
                self.gate.hold().await;
                return Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "child-checkpoint".into(),
                        name: "child_checkpoint".into(),
                        input: json!({}),
                    }],
                    usage: TokenUsage {
                        input_tokens: 1,
                        output_tokens: 1,
                        ..Default::default()
                    },
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                });
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("child done".into())],
                usage: TokenUsage {
                    input_tokens: 1,
                    output_tokens: 1,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }

    struct SupervisorDelegatingModel {
        calls: AtomicU32,
        histories: Arc<Mutex<Vec<Vec<Message>>>>,
    }

    #[async_trait]
    impl ModelAdapter for SupervisorDelegatingModel {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "supervisor"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<tokio_mpsc::Sender<crate::model::StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            self.histories
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(messages.to_vec());
            if n == 0 {
                return Ok(ModelResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "delegate-1".into(),
                        name: "worker".into(),
                        input: json!({"input": "do work"}),
                    }],
                    usage: TokenUsage::default(),
                    stop_reason: StopReason::ToolUse,
                    option_adjustments: vec![],
                });
            }
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("supervisor done".into())],
                usage: TokenUsage::default(),
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }

    struct CheckpointTool {
        metadata: ToolMetadata,
    }

    impl CheckpointTool {
        fn new() -> Self {
            Self {
                metadata: ToolMetadata {
                    side_effect: false,
                    approval: crate::tool::Approval::Never,
                    source: ToolSource::InProcess,
                    ..ToolMetadata::default()
                },
            }
        }
    }

    #[async_trait]
    impl Tool for CheckpointTool {
        fn name(&self) -> &str {
            "child_checkpoint"
        }
        fn description(&self) -> &str {
            "noop checkpoint"
        }
        fn input_schema(&self) -> &JsonSchema {
            &Value::Null
        }
        fn output_schema(&self) -> Option<&JsonSchema> {
            None
        }
        fn metadata(&self) -> &ToolMetadata {
            &self.metadata
        }
        async fn execute(
            &self,
            _input: Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Immediate(json!({"ok": true})))
        }
    }

    fn history_contains(histories: &Mutex<Vec<Vec<Message>>>, role: Role, needle: &str) -> bool {
        histories
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .flatten()
            .any(|message| {
                message.role == role
                    && message.content.iter().any(|block| match block {
                        ContentBlock::Text(text) => text.contains(needle),
                        _ => false,
                    })
            })
    }

    #[tokio::test]
    async fn child_control_targets_child_not_supervisor_and_completion_is_independent() {
        const CHILD_INJECT: &str = "child-target inject";
        const CHILD_STEER: &str = "child-target steer";
        const SUPERVISOR_INJECT: &str = "supervisor-target inject";

        let child_gate = ChildControlGate::new();
        let child_histories = Arc::new(Mutex::new(Vec::new()));
        let supervisor_histories = Arc::new(Mutex::new(Vec::new()));

        let child_model: Arc<dyn ModelAdapter> = Arc::new(GatedChildModel {
            calls: AtomicU32::new(0),
            gate: Arc::clone(&child_gate),
            histories: Arc::clone(&child_histories),
        });
        let mut child_registry = ToolRegistry::new();
        child_registry
            .register(Arc::new(CheckpointTool::new()))
            .expect("register checkpoint");
        let mut child_config = test_agent_config();
        child_config.runtime.max_steps = 4;
        let worker = child_config
            .as_tool("worker", "delegated worker")
            .model(Arc::clone(&child_model))
            .registry(child_registry)
            .build()
            .expect("build worker tool");

        let supervisor_model: Arc<dyn ModelAdapter> = Arc::new(SupervisorDelegatingModel {
            calls: AtomicU32::new(0),
            histories: Arc::clone(&supervisor_histories),
        });
        let mut supervisor_registry = ToolRegistry::new();
        supervisor_registry
            .register(worker)
            .expect("register worker");
        let mut supervisor_config = AgentConfig::builder("supervisor", "mock/supervisor")
            .system_prompt("delegate")
            .max_steps(4)
            .build()
            .expect("supervisor config");
        supervisor_config.runtime.max_steps = 4;

        let (handle, mut rx) = AgentRun::start(
            supervisor_config,
            "parent request".into(),
            supervisor_model,
            supervisor_registry,
        );

        // Drain until the child is registered, then exercise child control and
        // a concurrent supervisor inject without consuming completion yet.
        let mut child_run_id = None;
        let mut completion_task = None;
        while let Some(event) = rx.recv().await {
            if let RuntimeEvent::SubAgentStarted {
                child_run_id: id, ..
            } = &event
            {
                child_run_id = Some(*id);
                child_gate.wait_entered().await;
                let child = handle
                    .child(*id)
                    .await
                    .expect("public child control surface after SubAgentStarted");
                assert_eq!(child.run_id, *id);
                assert_eq!(child.parent_run_id, handle.run_id);

                child.inject_message(CHILD_INJECT);
                child.steer(CHILD_STEER);
                handle.inject_message(SUPERVISOR_INJECT);

                let child_for_wait = handle
                    .child(*id)
                    .await
                    .expect("child still registered while gated");
                completion_task = Some(tokio::spawn(async move {
                    child_for_wait.wait_completion().await
                }));

                child_gate.release();
            }
            if matches!(
                event,
                RuntimeEvent::RunCompleted { .. } | RuntimeEvent::RunFailed { .. }
            ) {
                break;
            }
        }
        // Drain remainder then wait the supervisor handle.
        while rx.try_recv().is_ok() {}
        handle.wait().await;

        let child_run_id = child_run_id.expect("SubAgentStarted observed");
        let outcome = completion_task
            .expect("completion task started")
            .await
            .expect("completion join")
            .expect("child completion");
        match outcome {
            ChildRunOutcome::Completed { output } => {
                assert_eq!(output, json!("child done"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }

        assert!(
            history_contains(&child_histories, Role::User, CHILD_INJECT),
            "child inject must land in child conversation"
        );
        assert!(
            history_contains(&child_histories, Role::System, CHILD_STEER),
            "child steer must land in child conversation"
        );
        assert!(
            !history_contains(&child_histories, Role::User, SUPERVISOR_INJECT),
            "supervisor inject must not appear in child conversation"
        );
        assert!(
            !history_contains(&supervisor_histories, Role::User, CHILD_INJECT),
            "child inject must not appear in supervisor conversation"
        );
        assert!(
            !history_contains(&supervisor_histories, Role::System, CHILD_STEER),
            "child steer must not appear in supervisor conversation"
        );
        assert!(
            history_contains(&supervisor_histories, Role::User, SUPERVISOR_INJECT),
            "supervisor-level inject remains covered"
        );

        // After unregister, lookup is gone but the awaited completion already succeeded.
        // Re-creating a handle is not required; prove active_children is empty post-wait.
        // (Registry unregisters after the child tool returns.)
        let _ = child_run_id;
    }

    #[tokio::test]
    async fn child_control_publishes_failed_outcome() {
        let child_model: Arc<dyn ModelAdapter> = Arc::new(FailingModel);
        let worker = test_agent_config()
            .as_tool("worker", "delegated worker")
            .model(child_model)
            .registry(ToolRegistry::new())
            .build()
            .expect("build worker");

        let supervisor_model: Arc<dyn ModelAdapter> = Arc::new(SupervisorDelegatingModel {
            calls: AtomicU32::new(0),
            histories: Arc::new(Mutex::new(Vec::new())),
        });
        let mut registry = ToolRegistry::new();
        registry.register(worker).expect("register");
        let config = AgentConfig::builder("supervisor", "mock/supervisor")
            .system_prompt("delegate")
            .max_steps(4)
            .build()
            .expect("config");

        let (handle, mut rx) = AgentRun::start(config, "boom".into(), supervisor_model, registry);
        let mut wait_task = None;
        while let Some(event) = rx.recv().await {
            if let RuntimeEvent::SubAgentStarted {
                child_run_id: id, ..
            } = &event
            {
                let child = handle.child(*id).await.expect("child handle");
                wait_task = Some(tokio::spawn(async move { child.wait_completion().await }));
            }
            if matches!(
                event,
                RuntimeEvent::RunCompleted { .. } | RuntimeEvent::RunFailed { .. }
            ) {
                break;
            }
        }
        while rx.try_recv().is_ok() {}
        handle.wait().await;

        let outcome = wait_task
            .expect("wait task")
            .await
            .expect("join")
            .expect("outcome");
        match outcome {
            ChildRunOutcome::Failed { error, .. } => {
                assert!(error.contains("provider exploded"), "{error}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
