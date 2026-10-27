//! Session event log: the append-only execution-state record of a run, and
//! the projection that derives the conversation, step and budget from it.
//!
//! The log is **execution state**. It is not the runtime event stream
//! ([`RuntimeEvent`](crate::events::RuntimeEvent)), which is an observation
//! stream for watchers and hosts; the two are kept separate on purpose.
//!
//! Every mutation of run state is recorded as a [`SessionEvent`] first and
//! folded into a [`SessionProjection`] by the same function a reader uses to
//! replay the log, so a run's live view and a reloaded log never disagree.

use serde::{Deserialize, Serialize};

use crate::budget::BudgetUsage;
use crate::events::RunFailureKind;
use crate::model::{ContentBlock, Message, Role, StopReason};
use crate::run::RunId;
use crate::tool::ToolCall;

use super::store::SessionError;

/// One entry of a session log. `seq` starts at 1 and increases by exactly one
/// per event; a log with a gap, a repeat or a step back is rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub seq: u64,
    /// Milliseconds since the Unix epoch when the event was recorded.
    pub timestamp_ms: u64,
    pub kind: SessionEventKind,
}

/// What a session event records.
///
/// Message-producing kinds (`SystemMessage`, `UserMessage`,
/// `AssistantMessage`, `ToolResult`, `Compacted`, `Handoff`) change the
/// projected conversation; the rest mark run and step boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionEventKind {
    /// A run began writing to this session.
    RunStarted { run_id: RunId, agent_name: String },
    /// A step began; the model request for `step` follows.
    StepStarted { step: u32 },
    /// A step finished. `budget_used` is the run's budget usage at the step
    /// boundary.
    StepEnded { step: u32, budget_used: BudgetUsage },
    /// A system message (the system prompt or a steer instruction).
    SystemMessage { content: Vec<ContentBlock> },
    /// A user turn.
    UserMessage { content: Vec<ContentBlock> },
    /// A model response, including any `ToolUse` blocks it requested.
    AssistantMessage { content: Vec<ContentBlock> },
    /// A tool call was dispatched to its tool.
    ToolCall { call: ToolCall },
    /// The result for one tool call. Consecutive results form the user
    /// message that follows the assistant message holding the `ToolUse`
    /// blocks.
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
    },
    /// Context compaction replaced the conversation with `messages`.
    Compacted { messages: Vec<Message> },
    /// A handoff moved the run to `to_agent`, whose conversation is
    /// `messages`.
    Handoff {
        from_agent: String,
        to_agent: String,
        messages: Vec<Message>,
        budget_used: BudgetUsage,
    },
    /// The run produced its final answer.
    RunEnded {
        output: serde_json::Value,
        stop_reason: StopReason,
    },
    /// The run failed.
    RunFailed { error: String, kind: RunFailureKind },
    /// The run was aborted.
    RunAborted { reason: Option<String> },
}

/// State derived from a session log.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionProjection {
    /// The conversation as the model sees it.
    pub messages: Vec<Message>,
    /// The next step to run.
    pub step: u32,
    /// Budget usage at the last step boundary.
    pub budget_used: BudgetUsage,
    /// Tool calls dispatched without a recorded result.
    pub pending: Vec<ToolCall>,
    /// The agent that owns the conversation (changes on handoff).
    pub agent_name: Option<String>,
    /// The run that last wrote to the session.
    pub run_id: Option<RunId>,
    /// Whether the last message-producing event was a `ToolResult`, so the
    /// next result joins the same user message.
    tool_results_open: bool,
}

impl SessionProjection {
    /// Projects a whole log.
    pub fn from_events(events: &[SessionEvent]) -> Self {
        let mut projection = Self::default();
        for event in events {
            projection.apply(event);
        }
        projection
    }

    /// Folds one event into the projection.
    pub fn apply(&mut self, event: &SessionEvent) {
        match &event.kind {
            SessionEventKind::RunStarted { run_id, agent_name } => {
                self.run_id = Some(*run_id);
                self.agent_name = Some(agent_name.clone());
            }
            SessionEventKind::StepStarted { .. } => {}
            SessionEventKind::StepEnded { step, budget_used } => {
                self.step = step + 1;
                self.budget_used = budget_used.clone();
            }
            SessionEventKind::SystemMessage { content } => {
                self.push_message(Role::System, content.clone());
            }
            SessionEventKind::UserMessage { content } => {
                self.push_message(Role::User, content.clone());
            }
            SessionEventKind::AssistantMessage { content } => {
                self.push_message(Role::Assistant, content.clone());
            }
            SessionEventKind::ToolCall { call } => {
                self.pending.push(call.clone());
            }
            SessionEventKind::ToolResult {
                tool_use_id,
                content,
            } => {
                self.pending.retain(|call| &call.id != tool_use_id);
                let block = ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                };
                match self.messages.last_mut() {
                    Some(last) if self.tool_results_open => last.content.push(block),
                    _ => self.messages.push(Message {
                        role: Role::User,
                        content: vec![block],
                    }),
                }
                self.tool_results_open = true;
            }
            SessionEventKind::Compacted { messages } => {
                self.messages = messages.clone();
                self.tool_results_open = false;
            }
            SessionEventKind::Handoff {
                to_agent,
                messages,
                budget_used,
                ..
            } => {
                self.messages = messages.clone();
                self.agent_name = Some(to_agent.clone());
                self.budget_used = budget_used.clone();
                self.tool_results_open = false;
            }
            SessionEventKind::RunEnded { .. }
            | SessionEventKind::RunFailed { .. }
            | SessionEventKind::RunAborted { .. } => {}
        }
    }

    fn push_message(&mut self, role: Role, content: Vec<ContentBlock>) {
        self.messages.push(Message { role, content });
        self.tool_results_open = false;
    }
}

/// Checks that `events` continue a log whose next sequence number is
/// `next_seq`: strictly consecutive, starting at `next_seq`.
pub fn check_sequence(next_seq: u64, events: &[SessionEvent]) -> Result<(), SessionError> {
    for (expected, event) in (next_seq..).zip(events) {
        if event.seq != expected {
            return Err(SessionError::SequenceConflict {
                expected,
                found: event.seq,
            });
        }
    }
    Ok(())
}

/// A session log held by a run: the events recorded so far, their
/// projection, and how many of them a store has accepted.
#[derive(Debug, Clone, Default)]
pub struct SessionLog {
    events: Vec<SessionEvent>,
    projection: SessionProjection,
    persisted: usize,
}

impl SessionLog {
    /// An empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds a log from stored events (all of them already persisted).
    pub fn from_events(events: Vec<SessionEvent>) -> Result<Self, SessionError> {
        check_sequence(1, &events)?;
        let projection = SessionProjection::from_events(&events);
        let persisted = events.len();
        Ok(Self {
            events,
            projection,
            persisted,
        })
    }

    /// Records `kind` as the next event and folds it into the projection.
    pub fn append(&mut self, kind: SessionEventKind) -> &SessionEvent {
        let event = SessionEvent {
            seq: self.next_seq(),
            timestamp_ms: now_ms(),
            kind,
        };
        self.projection.apply(&event);
        self.events.push(event);
        self.events.last().expect("just pushed")
    }

    /// Records a message under the event kind matching its role.
    pub fn append_message(&mut self, message: Message) {
        let Message { role, content } = message;
        let kind = match role {
            Role::System => SessionEventKind::SystemMessage { content },
            Role::Assistant => SessionEventKind::AssistantMessage { content },
            _ => SessionEventKind::UserMessage { content },
        };
        self.append(kind);
    }

    pub fn events(&self) -> &[SessionEvent] {
        &self.events
    }

    pub fn projection(&self) -> &SessionProjection {
        &self.projection
    }

    pub fn messages(&self) -> &[Message] {
        &self.projection.messages
    }

    pub fn step(&self) -> u32 {
        self.projection.step
    }

    /// The sequence number the next event will get.
    pub fn next_seq(&self) -> u64 {
        self.events.len() as u64 + 1
    }

    /// Events not yet accepted by a store.
    pub fn unpersisted(&self) -> &[SessionEvent] {
        &self.events[self.persisted..]
    }

    /// Marks every recorded event as accepted by a store.
    pub fn mark_persisted(&mut self) {
        self.persisted = self.events.len();
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(seq: u64, kind: SessionEventKind) -> SessionEvent {
        SessionEvent {
            seq,
            timestamp_ms: 0,
            kind,
        }
    }

    fn user(text: &str) -> SessionEventKind {
        SessionEventKind::UserMessage {
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    #[test]
    fn sequence_must_be_consecutive() {
        let ok = vec![event(1, user("a")), event(2, user("b"))];
        assert!(check_sequence(1, &ok).is_ok());

        let gap = vec![event(1, user("a")), event(3, user("b"))];
        assert!(matches!(
            check_sequence(1, &gap),
            Err(SessionError::SequenceConflict {
                expected: 2,
                found: 3
            })
        ));

        let repeat = vec![event(1, user("a")), event(1, user("b"))];
        assert!(check_sequence(1, &repeat).is_err());

        let back = vec![event(2, user("a")), event(1, user("b"))];
        assert!(check_sequence(2, &back).is_err());
        assert!(SessionLog::from_events(back).is_err());
    }

    #[test]
    fn append_assigns_consecutive_seq() {
        let mut log = SessionLog::new();
        log.append(user("a"));
        log.append(user("b"));
        let seqs: Vec<u64> = log.events().iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2]);
        assert_eq!(log.next_seq(), 3);
        assert_eq!(log.unpersisted().len(), 2);
        log.mark_persisted();
        assert!(log.unpersisted().is_empty());
    }

    #[test]
    fn projection_pairs_tool_use_and_results() {
        let mut log = SessionLog::new();
        log.append(SessionEventKind::SystemMessage {
            content: vec![ContentBlock::Text("sys".into())],
        });
        log.append(user("hi"));
        log.append(SessionEventKind::StepStarted { step: 0 });
        log.append(SessionEventKind::AssistantMessage {
            content: vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "a".into(),
                    input: json!({}),
                },
                ContentBlock::ToolUse {
                    id: "t2".into(),
                    name: "b".into(),
                    input: json!({}),
                },
            ],
        });
        log.append(SessionEventKind::ToolCall {
            call: ToolCall {
                id: "t1".into(),
                name: "a".into(),
                input: json!({}),
            },
        });
        log.append(SessionEventKind::ToolResult {
            tool_use_id: "t1".into(),
            content: json!("one"),
        });
        log.append(SessionEventKind::ToolCall {
            call: ToolCall {
                id: "t2".into(),
                name: "b".into(),
                input: json!({}),
            },
        });
        assert_eq!(log.projection().pending.len(), 1);
        log.append(SessionEventKind::ToolResult {
            tool_use_id: "t2".into(),
            content: json!("two"),
        });
        log.append(SessionEventKind::StepEnded {
            step: 0,
            budget_used: BudgetUsage {
                tokens_used: 10,
                tool_calls_used: 2,
                cost_usd: 0.0,
            },
        });

        let projection = log.projection();
        assert_eq!(projection.messages.len(), 4);
        assert_eq!(projection.messages[2].role, Role::Assistant);
        assert_eq!(projection.messages[3].role, Role::User);
        let ids: Vec<&str> = projection.messages[3]
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["t1", "t2"]);
        assert!(projection.pending.is_empty());
        assert_eq!(projection.step, 1);
        assert_eq!(projection.budget_used.tool_calls_used, 2);
    }

    #[test]
    fn replayed_log_projects_like_the_live_log() {
        let mut log = SessionLog::new();
        log.append(user("hi"));
        log.append(SessionEventKind::AssistantMessage {
            content: vec![ContentBlock::Text("hello".into())],
        });
        log.append(SessionEventKind::Compacted {
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text("summary".into())],
            }],
        });
        log.append(user("next"));
        let replayed = SessionLog::from_events(log.events().to_vec()).unwrap();
        assert_eq!(replayed.projection(), log.projection());
        assert_eq!(replayed.messages().len(), 2);
        assert!(replayed.unpersisted().is_empty());
    }

    #[test]
    fn event_kind_is_tagged_on_the_wire() {
        let json = serde_json::to_value(event(1, user("a"))).unwrap();
        assert_eq!(json["kind"]["type"], "user_message");
        let back: SessionEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, event(1, user("a")));
    }
}
