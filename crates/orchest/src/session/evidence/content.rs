use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{store::encode, EvidenceError, EvidenceKind, EvidenceRecord, RunEvidence};
use crate::model::{ContentBlock, Message, RequestOptions, ToolDef};
use crate::run::RunId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceField {
    Messages,
    Tools,
    Options,
    Response,
    OptionAdjustments,
    CommittedResponse,
    StreamText,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Omission {
    Policy,
    Restricted,
    Limit,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "coverage", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapturedField {
    Exact(Value),
    Omitted(Omission),
}

type Authorize = dyn Fn(EvidenceField, &Value) -> bool + Send + Sync;

/// Host authorization of structurally validated, bounded fields. The callback
/// runs synchronously at the adapter boundary and must be fast and free of I/O.
/// Inspect arbitrary text and JSON for host-specific secrets before allowing it.
#[derive(Clone)]
pub struct EvidenceContentPolicy(Arc<Authorize>);

impl EvidenceContentPolicy {
    pub fn new(authorize: impl Fn(EvidenceField, &Value) -> bool + Send + Sync + 'static) -> Self {
        Self(Arc::new(authorize))
    }

    pub(super) fn capture(
        &self,
        field: EvidenceField,
        input: &impl Serialize,
        max_bytes: usize,
        check: impl FnOnce(&mut CaptureBudget) -> Result<(), Omission>,
    ) -> CapturedField {
        match self.exact(field, input, max_bytes, check) {
            Ok(value) => CapturedField::Exact(value),
            Err(reason) => CapturedField::Omitted(reason),
        }
    }

    fn exact(
        &self,
        field: EvidenceField,
        input: &impl Serialize,
        max_bytes: usize,
        check: impl FnOnce(&mut CaptureBudget) -> Result<(), Omission>,
    ) -> Result<Value, Omission> {
        // Leave room for the attempt, scope and evidence envelope.
        let limit = max_bytes.saturating_sub(512);
        check(&mut CaptureBudget::new(limit))?;
        let bytes = encode(input, limit).map_err(|_| Omission::Limit)?;
        let value = serde_json::from_slice(&bytes).map_err(|_| Omission::Unsupported)?;
        // A host callback failure cannot change the outcome of a model call.
        let allowed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.0)(field, &value)))
                .unwrap_or(false);
        if allowed {
            Ok(value)
        } else {
            Err(Omission::Policy)
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AdapterInput {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub options: RequestOptions,
}

/// Rebuild the structured input from the records for one attempt. Pass all
/// relevant pages. Missing or withheld fields make reconstruction incomplete.
pub fn reconstruct_input(
    records: &[EvidenceRecord],
    attempt_id: Uuid,
) -> Result<AdapterInput, EvidenceError> {
    Ok(AdapterInput {
        messages: exact_field(records, attempt_id, EvidenceField::Messages)?,
        tools: exact_field(records, attempt_id, EvidenceField::Tools)?,
        options: exact_field(records, attempt_id, EvidenceField::Options)?,
    })
}

fn exact_field<T: serde::de::DeserializeOwned>(
    records: &[EvidenceRecord],
    attempt_id: Uuid,
    field: EvidenceField,
) -> Result<T, EvidenceError> {
    let mut values = records.iter().filter_map(|record| match &record.kind {
        EvidenceKind::Field {
            attempt_id: id,
            field: candidate,
            capture,
        } if *id == attempt_id && *candidate == field => Some(capture),
        _ => None,
    });
    match (values.next(), values.next()) {
        (Some(CapturedField::Exact(value)), None) => {
            T::deserialize(value).map_err(|_| EvidenceError::InvalidRecord)
        }
        _ => Err(EvidenceError::IncompleteInput { field }),
    }
}

impl RunEvidence {
    #[allow(clippy::too_many_arguments)] // justified: Summary prompts also need source-policy validation.
    pub(crate) fn input(
        &self,
        run_id: RunId,
        step: u32,
        attempt_id: Uuid,
        messages: &[Message],
        tools: &[ToolDef],
        options: &RequestOptions,
        source_messages: Option<&[Message]>,
    ) {
        self.capture_field(
            run_id,
            step,
            attempt_id,
            EvidenceField::Messages,
            &messages,
            |budget| {
                if let Some(source) = source_messages {
                    for message in source {
                        budget.charge(8)?;
                        budget.blocks(&message.content)?;
                    }
                }
                for message in messages {
                    budget.charge(8)?;
                    budget.blocks(&message.content)?;
                }
                Ok(())
            },
        );
        self.capture_field(
            run_id,
            step,
            attempt_id,
            EvidenceField::Tools,
            &tools,
            |budget| {
                for tool in tools {
                    budget.text(&tool.name)?;
                    budget.text(&tool.description)?;
                    budget.json(&tool.input_schema, 0)?;
                }
                Ok(())
            },
        );
        self.capture_field(
            run_id,
            step,
            attempt_id,
            EvidenceField::Options,
            options,
            |budget| {
                if let Some(tier) = &options.service_tier {
                    budget.text(tier)?;
                }
                if options.temperature.is_some_and(|v| !v.is_finite())
                    || options.top_p.is_some_and(|v| !v.is_finite())
                {
                    return Err(Omission::Unsupported);
                }
                Ok(())
            },
        );
    }

    #[allow(clippy::too_many_arguments)] // justified: Capture accepts a borrowed field with its own validation budget.
    pub(super) fn capture_field(
        &self,
        run_id: RunId,
        step: u32,
        attempt_id: Uuid,
        field: EvidenceField,
        input: &impl Serialize,
        check: impl FnOnce(&mut CaptureBudget) -> Result<(), Omission>,
    ) {
        let Some(policy) = &self.content else { return };
        self.database.record_with(self.instance, || {
            let capture = policy.capture(field, input, self.database.record_bytes(), check);
            self.make_record(
                run_id,
                step,
                EvidenceKind::Field {
                    attempt_id,
                    field,
                    capture,
                },
            )
        });
    }
}

pub(super) struct CaptureBudget(usize);

impl CaptureBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self(limit)
    }

    pub(super) fn charge(&mut self, bytes: usize) -> Result<(), Omission> {
        self.0 = self.0.checked_sub(bytes).ok_or(Omission::Limit)?;
        Ok(())
    }

    pub(super) fn text(&mut self, text: &str) -> Result<(), Omission> {
        self.charge(text.len().saturating_add(4))?;
        if restricted_text(text) {
            return Err(Omission::Restricted);
        }
        Ok(())
    }

    pub(super) fn json(&mut self, value: &Value, depth: usize) -> Result<(), Omission> {
        if depth > 32 {
            return Err(Omission::Limit);
        }
        self.charge(4)?;
        match value {
            Value::String(text) => self.text(text)?,
            Value::Array(items) => {
                for item in items {
                    self.json(item, depth + 1)?;
                }
            }
            Value::Object(fields) => {
                for (key, value) in fields {
                    self.text(key)?;
                    if [
                        "api_key",
                        "apikey",
                        "authorization",
                        "password",
                        "secret",
                        "access_token",
                        "refresh_token",
                        "private_key",
                    ]
                    .iter()
                    .any(|name| key.eq_ignore_ascii_case(name))
                    {
                        return Err(Omission::Restricted);
                    }
                    self.json(value, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn blocks(&mut self, blocks: &[ContentBlock]) -> Result<(), Omission> {
        for block in blocks {
            self.charge(8)?;
            match block {
                ContentBlock::Text(text) | ContentBlock::MidConvSystem(text) => self.text(text)?,
                ContentBlock::ToolUse { id, name, input } => {
                    self.text(id)?;
                    self.text(name)?;
                    self.json(input, 0)?;
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                } => {
                    self.text(tool_use_id)?;
                    self.json(content, 0)?;
                }
                // Private reasoning and attachment content are outside this policy.
                _ => return Err(Omission::Restricted),
            }
        }
        Ok(())
    }
}

pub(super) fn restricted_text(text: &str) -> bool {
    ["-----BEGIN ", "sk-proj-", "sk-ant-", "ghp_", "github_pat_"]
        .iter()
        .any(|marker| text.contains(marker))
        || text
            .as_bytes()
            .windows(7)
            .any(|prefix| prefix.eq_ignore_ascii_case(b"bearer "))
}
