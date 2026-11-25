use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    AttemptObservation, AttemptOutcome, AttemptPurpose, EvidenceEventRef, EvidenceKind, RunEvidence,
};
use crate::model::{Message, ModelAdapter, RequestOptions, Role};
use crate::run::RunId;
use crate::session::{SessionEvent, SessionEventKind};

const MAX_LINEAGE_POSITIONS: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CompactionOutcome {
    HookBlocked,
    NoSafeBoundary,
    SummaryFailed,
    SummaryReturned,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MessageOrigin {
    /// Zero-based position in the projection through CompactionStarted.through_seq.
    PreviousPosition(usize),
    SummaryAttempt(Uuid),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectionOrigin {
    SessionMessage {
        seq: u64,
    },
    /// Use ToolResult events in this sequence range, in log order.
    GroupedToolResults {
        first_seq: u64,
        last_seq: u64,
    },
    /// Position within a Handoff or Compacted event's replacement messages.
    Replacement {
        seq: u64,
        position: usize,
    },
}

/// Resolve a zero-based position in a native v2 projection to its source
/// event or replacement slot. Pass the validated v2 log from sequence 1.
/// Compare a Compacted source with its verified CompactionApplied evidence
/// before following it to the previous position or summary attempt.
pub fn projection_origin(
    events: &[SessionEvent],
    through_seq: u64,
    position: usize,
) -> Option<ProjectionOrigin> {
    let mut len = 0;
    let mut tools_open = false;
    let mut origin = None;
    if through_seq > events.last()?.seq {
        return None;
    }
    for event in events.iter().take_while(|event| event.seq <= through_seq) {
        let seq = event.seq;
        match &event.kind {
            SessionEventKind::UserMessage { .. }
            | SessionEventKind::SystemMessage { .. }
            | SessionEventKind::AssistantMessage { .. } => {
                if len == position {
                    origin = Some(ProjectionOrigin::SessionMessage { seq });
                }
                len += 1;
                tools_open = false;
            }
            SessionEventKind::ToolResult { .. } => {
                if tools_open {
                    if let Some(ProjectionOrigin::GroupedToolResults { last_seq, .. }) = &mut origin
                    {
                        if position == len.saturating_sub(1) {
                            *last_seq = seq;
                        }
                    }
                } else {
                    if len == position {
                        origin = Some(ProjectionOrigin::GroupedToolResults {
                            first_seq: seq,
                            last_seq: seq,
                        });
                    }
                    len += 1;
                    tools_open = true;
                }
            }
            SessionEventKind::Compacted { messages }
            | SessionEventKind::Handoff { messages, .. } => {
                len = messages.len();
                origin =
                    (position < len).then_some(ProjectionOrigin::Replacement { seq, position });
                tools_open = false;
            }
            _ => {}
        }
    }
    origin
}

pub(crate) struct CompactionObservation {
    evidence: RunEvidence,
    run_id: RunId,
    step: u32,
    through_seq: u64,
    id: Option<Uuid>,
    summary: Option<AttemptObservation>,
    outcome: Option<CompactionOutcome>,
    mapping: Option<Vec<MessageOrigin>>,
    summarized: Option<Vec<usize>>,
}

impl CompactionObservation {
    pub(crate) fn new(evidence: RunEvidence, run_id: RunId, step: u32, through_seq: u64) -> Self {
        Self {
            evidence,
            run_id,
            step,
            through_seq,
            id: None,
            summary: None,
            outcome: None,
            mapping: None,
            summarized: None,
        }
    }

    pub(crate) fn start(&mut self) {
        let compaction_id = Uuid::new_v4();
        self.id = Some(compaction_id);
        self.evidence.record(
            self.run_id,
            self.step,
            EvidenceKind::CompactionStarted {
                compaction_id,
                through_seq: self.through_seq,
            },
        );
    }

    pub(crate) fn start_summary(
        &mut self,
        model: &dyn ModelAdapter,
        input: &[Message],
        options: &RequestOptions,
        source: &[Message],
    ) -> &AttemptObservation {
        let attempt = self.summary.insert(AttemptObservation::start(
            &self.evidence,
            (self.run_id, self.step),
            0,
            AttemptPurpose::CompactionSummary,
            model,
        ));
        self.evidence.input(
            self.run_id,
            self.step,
            attempt.id,
            input,
            &[],
            options,
            Some(source),
        );
        attempt
    }

    pub(crate) fn outcome(&mut self, outcome: CompactionOutcome) {
        self.outcome = Some(outcome);
    }

    pub(crate) fn record_selection(
        &mut self,
        messages: &[Message],
        split_at: usize,
        hook_may_rewrite: bool,
    ) {
        let Some(summary) = &self.summary else { return };
        if messages.len() > MAX_LINEAGE_POSITIONS {
            return;
        }
        let origin = |position| {
            if hook_may_rewrite {
                MessageOrigin::Unknown
            } else {
                MessageOrigin::PreviousPosition(position)
            }
        };
        let mut mapping = Vec::new();
        if let Some(position) = messages
            .iter()
            .position(|message| message.role == Role::System)
        {
            mapping.push(origin(position));
        }
        mapping.push(MessageOrigin::SummaryAttempt(summary.id));
        let non_system = messages
            .iter()
            .enumerate()
            .filter(|(_, message)| message.role != Role::System)
            .map(|(position, _)| position);
        if !hook_may_rewrite {
            self.summarized = Some(non_system.clone().take(split_at).collect());
        }
        mapping.extend(non_system.skip(split_at).map(origin));
        self.mapping = Some(mapping);
    }

    pub(crate) fn finish(&self, cancelled: bool) {
        let Some(compaction_id) = self.id else { return };
        let outcome = if cancelled {
            if self.outcome.is_none() {
                if let Some(summary) = &self.summary {
                    summary.finish(AttemptOutcome::Cancelled);
                }
            }
            CompactionOutcome::Cancelled
        } else {
            let Some(outcome) = self.outcome else { return };
            outcome
        };
        self.evidence.record(
            self.run_id,
            self.step,
            EvidenceKind::CompactionFinished {
                compaction_id,
                outcome,
                summary_attempt: self.summary.as_ref().map(|summary| summary.id),
            },
        );
    }

    pub(crate) fn applied(self, event: &SessionEvent) {
        let Some(compaction_id) = self.id else { return };
        let Some(event) = EvidenceEventRef::from_event(event) else {
            return;
        };
        self.evidence.record(
            self.run_id,
            self.step,
            EvidenceKind::CompactionApplied {
                compaction_id,
                event,
                previous_through_seq: self.through_seq,
                mapping: self.mapping,
                summarized: self.summarized,
            },
        );
    }
}
