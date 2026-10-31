//! Dialect-agnostic ASR contract (ADR-0006, v1.2 Issue 003).
//!
//! Each streaming ASR dialect's real session loop runs against a recorded
//! WebSocket fixture (`tests/fixtures/asr/<dialect>/*.jsonl`, replayed by
//! [`orchest_provider_stream::fixture`]), and one assertion set checks the
//! events it emits:
//!
//! - every Draft is a `Snapshot` with a segment id, and no Draft follows a
//!   Stable on the same segment;
//! - every Stable has a segment id;
//! - every Turn points at a Stable already seen, once per segment, or is
//!   segment-less only when it answers a `Finalize`;
//! - every Stable segment is ended by a Turn before the session closes, so
//!   no confirmed text is left without the Turn that hands it on;
//! - every `Finalize` gets exactly one result: a Turn (`client_finalize =
//!   true`, under a normal fixture) or a non-fatal `UnsupportedOption` error
//!   tagged `operation = "finalize"` (`client_finalize = false`), and the
//!   session keeps taking audio afterwards;
//! - the session ends with `SessionClosed`, and nothing follows it.
//!
//! The fixtures replay the same utterances, so the Stable text per Turn must
//! match across dialects too. Re-record with `scripts/record-asr-fixture.sh`.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use orchest_protocol::{
    ErrorCode, LifecycleEvent, SegmentRef, SessionInput, StreamEvent, TranscriptStability,
    TranscriptUpdateKind,
};
use orchest_provider_stream::asr::{
    aliyun, deepgram, elevenlabs, soniox, volcengine, FINALIZE_OPERATION,
};
use orchest_provider_stream::fixture::{parse_fixture, replay, FixtureInput, ReplayDuplex};
use tokio::sync::{mpsc, oneshot};

/// What the client did and what the session emitted, in order.
#[derive(Debug)]
enum Observed {
    Audio,
    Finalize,
    Event(StreamEvent),
}

/// The Stable text of each Turn every fixture records.
const EXPECTED_TURNS: [&str; 2] = ["hello world", "how are you"];

/// Replay `fixture` through `run` and return what was observed plus any uplink
/// frames the fixture did not expect.
async fn replay_session<F, Fut>(fixture: &str, run: F) -> (Vec<Observed>, Vec<String>)
where
    F: FnOnce(ReplayDuplex, mpsc::Receiver<SessionInput>, mpsc::Sender<StreamEvent>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let path = format!(
        "{}/tests/fixtures/asr/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let (duplex, mut inputs) = replay(parse_fixture(&text).expect("fixture parses"));
    let violations = duplex.violations();
    let (input_tx, input_rx) = mpsc::channel(32);
    let (events_tx, mut events_rx) = mpsc::channel(64);
    let log = Arc::new(Mutex::new(Vec::new()));

    let session = tokio::spawn(run(duplex, input_rx, events_tx));
    let collector = {
        let log = log.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                log.lock().unwrap().push(Observed::Event(event));
            }
        })
    };
    let driver = {
        let log = log.clone();
        async move {
            let mut input = Some(input_tx);
            while let Some(next) = inputs.next().await {
                let Some(tx) = input.as_ref() else { break };
                let message = match next {
                    FixtureInput::Audio { bytes } => {
                        log.lock().unwrap().push(Observed::Audio);
                        SessionInput::Audio(Bytes::from(vec![0; bytes]))
                    }
                    FixtureInput::Finalize => {
                        log.lock().unwrap().push(Observed::Finalize);
                        SessionInput::Finalize
                    }
                    FixtureInput::End => {
                        input = None;
                        continue;
                    }
                };
                if tx.send(message).await.is_err() {
                    break;
                }
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        driver.await;
        session.await.expect("session task");
        collector.await.expect("collector task");
    })
    .await
    .unwrap_or_else(|_| panic!("{fixture}: replay stalled; {:?}", violations.take()));

    let observed = std::mem::take(&mut *log.lock().unwrap());
    (observed, violations.take())
}

fn segment_id(segment: &Option<SegmentRef>) -> Option<&str> {
    segment.as_ref().and_then(|s| s.segment_id.as_deref())
}

/// The ADR-0006 contract over one observed session; returns every breach.
fn contract_breaches(client_finalize: bool, observed: &[Observed]) -> Vec<String> {
    let mut breaches = Vec::new();
    let mut stable: HashSet<String> = HashSet::new();
    let mut turned: HashSet<String> = HashSet::new();
    let mut pending_finalize = 0usize;
    let mut finalizes_sent = 0usize;
    let mut segment_less_turns = 0usize;
    let mut finalize_errors = 0usize;
    // Finalizes not yet followed by more client audio.
    let mut awaiting_audio = 0usize;
    let mut closed = false;
    for (index, item) in observed.iter().enumerate() {
        let event = match item {
            Observed::Audio => {
                awaiting_audio = 0;
                continue;
            }
            Observed::Finalize => {
                pending_finalize += 1;
                finalizes_sent += 1;
                awaiting_audio += 1;
                continue;
            }
            Observed::Event(event) => event,
        };
        if closed {
            breaches.push(format!("#{index}: event after SessionClosed: {event:?}"));
        }
        match event {
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                segment,
                ..
            } => match segment {
                Some(SegmentRef {
                    segment_id: Some(id),
                    update_kind: TranscriptUpdateKind::Snapshot,
                }) if stable.contains(id) => {
                    breaches.push(format!("#{index}: Draft on {id} after its Stable"))
                }
                Some(SegmentRef {
                    segment_id: Some(_),
                    update_kind: TranscriptUpdateKind::Snapshot,
                }) => {}
                other => breaches.push(format!(
                    "#{index}: Draft must be a Snapshot with a segment id, got {other:?}"
                )),
            },
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                segment,
                ..
            } => match segment_id(segment) {
                Some(id) => {
                    stable.insert(id.to_string());
                }
                None => breaches.push(format!("#{index}: Stable without a segment id")),
            },
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { segment }) => {
                match segment_id(segment) {
                    Some(id) if !stable.contains(id) => breaches.push(format!(
                        "#{index}: Turn points at {id}, which has no Stable"
                    )),
                    Some(id) if !turned.insert(id.to_string()) => {
                        breaches.push(format!("#{index}: second Turn for {id}"))
                    }
                    Some(_) => {}
                    // Only a dialect's Finalize answer is segment-less, and at
                    // most once per Finalize. The stream does not say which
                    // Turn answered, so a Turn ending Stable text may too.
                    None => {
                        segment_less_turns += 1;
                        if !client_finalize || segment_less_turns > finalizes_sent {
                            breaches.push(format!(
                                "#{index}: segment-less Turn that answers no Finalize"
                            ));
                        }
                    }
                }
                if client_finalize && pending_finalize > 0 {
                    pending_finalize -= 1;
                }
            }
            StreamEvent::Error { error, fatal } => {
                let answers_finalize = error.diagnostic_metadata["operation"] == FINALIZE_OPERATION;
                if *fatal {
                    breaches.push(format!("#{index}: fatal error {error:?}"));
                } else if !answers_finalize {
                    breaches.push(format!("#{index}: unexpected error {error:?}"));
                } else {
                    finalize_errors += 1;
                    // A supported dialect's Finalize may be answered after an
                    // ordinary Turn (by a timeout or an upstream rejection),
                    // so only the count is checked; an unsupported one answers
                    // each Finalize with this error alone.
                    let unanswered = if client_finalize {
                        finalize_errors > finalizes_sent
                    } else {
                        pending_finalize == 0
                    };
                    if unanswered {
                        breaches.push(format!(
                            "#{index}: finalize error that answers no Finalize: {error:?}"
                        ));
                    }
                    pending_finalize = pending_finalize.saturating_sub(1);
                    if !client_finalize && error.code != ErrorCode::UnsupportedOption {
                        breaches.push(format!(
                            "#{index}: unsupported Finalize must answer UnsupportedOption, \
                             got {error:?}"
                        ));
                    }
                }
            }
            StreamEvent::Lifecycle(LifecycleEvent::SessionClosed) => closed = true,
            _ => {}
        }
    }
    let mut unended: Vec<_> = stable.difference(&turned).cloned().collect();
    unended.sort();
    for id in unended {
        breaches.push(format!("Stable {id} was never ended by a Turn"));
    }
    if pending_finalize > 0 {
        breaches.push(format!(
            "{pending_finalize} Finalize(s) got neither a Turn nor a finalize error"
        ));
    }
    if awaiting_audio > 0 {
        breaches.push("no audio after the last Finalize: the session did not continue".into());
    }
    if !closed {
        breaches.push("session did not end with SessionClosed".into());
    }
    breaches
}

/// Under a normal fixture (the upstream answers every `Finalize` in time), a
/// `client_finalize = true` dialect answers with a Turn: a finalize error
/// (a timeout or an upstream rejection) is a breach there, though the
/// contract allows it in general.
fn normal_fixture_breaches(observed: &[Observed]) -> Vec<String> {
    observed
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            Observed::Event(StreamEvent::Error { error, .. })
                if error.diagnostic_metadata["operation"] == FINALIZE_OPERATION =>
            {
                Some(format!(
                    "#{index}: Finalize answered with an error under a normal fixture: {error:?}"
                ))
            }
            _ => None,
        })
        .collect()
}

/// The Stable text each Turn ended, applying `Append` / `Snapshot` updates.
fn turn_texts(observed: &[Observed]) -> Vec<String> {
    let mut segments: Vec<(String, String)> = Vec::new();
    let mut turns = Vec::new();
    for item in observed {
        match item {
            Observed::Event(StreamEvent::Transcript {
                text,
                stability: TranscriptStability::Committed,
                segment: Some(segment),
            }) => {
                let id = segment.segment_id.clone().unwrap_or_default();
                let slot = match segments.iter().position(|(seen, _)| *seen == id) {
                    Some(position) => &mut segments[position].1,
                    None => {
                        segments.push((id, String::new()));
                        &mut segments.last_mut().unwrap().1
                    }
                };
                match segment.update_kind {
                    TranscriptUpdateKind::Append => slot.push_str(text),
                    _ => *slot = text.clone(),
                }
            }
            Observed::Event(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                segment: Some(segment),
            })) => {
                let id = segment.segment_id.as_deref().unwrap_or_default();
                if let Some((_, text)) = segments.iter().find(|(seen, _)| seen == id) {
                    turns.push(text.trim().to_string());
                }
            }
            _ => {}
        }
    }
    turns
}

fn assert_contract(
    dialect: &str,
    client_finalize: bool,
    (observed, violations): (Vec<Observed>, Vec<String>),
) {
    assert!(violations.is_empty(), "{dialect}: replay: {violations:#?}");
    let mut breaches = contract_breaches(client_finalize, &observed);
    if client_finalize {
        breaches.extend(normal_fixture_breaches(&observed));
    }
    assert!(
        breaches.is_empty(),
        "{dialect}: {breaches:#?}\nobserved: {observed:#?}"
    );
    assert_eq!(
        turn_texts(&observed),
        EXPECTED_TURNS,
        "{dialect}: {observed:#?}"
    );
}

#[tokio::test]
async fn deepgram_meets_the_contract() {
    let result = replay_session("deepgram/hello.jsonl", deepgram::run_deepgram_stream).await;
    assert_contract("deepgram", deepgram::CLIENT_FINALIZE, result);
}

#[tokio::test]
async fn elevenlabs_meets_the_contract() {
    let result = replay_session("elevenlabs/hello.jsonl", |duplex, input, events| {
        elevenlabs::run_elevenlabs_stream(duplex, 16_000, input, events)
    })
    .await;
    assert_contract("elevenlabs", elevenlabs::CLIENT_FINALIZE, result);
}

#[tokio::test]
async fn soniox_meets_the_contract() {
    let result = replay_session("soniox/hello.jsonl", |duplex, input, events| {
        soniox::run_soniox_stream(duplex, "{}".into(), input, events)
    })
    .await;
    assert_contract("soniox", soniox::CLIENT_FINALIZE, result);
}

#[tokio::test]
async fn aliyun_meets_the_contract() {
    let result = replay_session("aliyun/hello.jsonl", |duplex, input, events| {
        let (ready, _ready_rx) = oneshot::channel();
        aliyun::run_aliyun_stream(
            duplex,
            "run-task".into(),
            "finish-task".into(),
            input,
            events,
            ready,
        )
    })
    .await;
    assert_contract("aliyun", aliyun::CLIENT_FINALIZE, result);
}

#[tokio::test]
async fn volcengine_meets_the_contract() {
    let result = replay_session("volcengine/hello.jsonl", volcengine::run_asr_stream).await;
    assert_contract("volcengine", volcengine::CLIENT_FINALIZE, result);
}

/// The Finalize receipt rules: one tagged answer per Finalize, of the right
/// kind for the dialect's `client_finalize`.
#[test]
fn finalize_receipt_breaches_are_detected() {
    let finalize_error = |code| {
        Observed::Event(StreamEvent::Error {
            error: orchest_protocol::ProtocolError {
                diagnostic_metadata: serde_json::json!({ "operation": FINALIZE_OPERATION }),
                ..orchest_protocol::ProtocolError::new(code, "x")
            },
            fatal: false,
        })
    };
    let untagged = Observed::Event(StreamEvent::Error {
        error: orchest_protocol::ProtocolError::new(ErrorCode::UnsupportedOption, "x"),
        fatal: false,
    });
    let closed = || Observed::Event(StreamEvent::Lifecycle(LifecycleEvent::SessionClosed));
    let bare_turn = || {
        Observed::Event(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment: None,
        }))
    };
    let cases: Vec<(bool, Vec<Observed>, &str)> = vec![
        (
            true,
            vec![
                Observed::Finalize,
                finalize_error(ErrorCode::Timeout),
                finalize_error(ErrorCode::Timeout),
                Observed::Audio,
                closed(),
            ],
            "answers no Finalize",
        ),
        (
            true,
            vec![
                Observed::Finalize,
                Observed::Audio,
                bare_turn(),
                bare_turn(),
                closed(),
            ],
            "segment-less Turn that answers no Finalize",
        ),
        (
            false,
            vec![
                Observed::Finalize,
                finalize_error(ErrorCode::Timeout),
                Observed::Audio,
                closed(),
            ],
            "must answer UnsupportedOption",
        ),
        (
            false,
            vec![Observed::Finalize, untagged, Observed::Audio, closed()],
            "unexpected error",
        ),
        (
            false,
            vec![finalize_error(ErrorCode::UnsupportedOption), closed()],
            "answers no Finalize",
        ),
    ];
    for (client_finalize, observed, needle) in cases {
        let breaches = contract_breaches(client_finalize, &observed);
        assert!(
            breaches.iter().any(|b| b.contains(needle)),
            "missing {needle:?} in {breaches:#?}"
        );
    }
    let ok = vec![
        Observed::Finalize,
        finalize_error(ErrorCode::UnsupportedOption),
        Observed::Audio,
        closed(),
    ];
    assert!(contract_breaches(false, &ok).is_empty());

    let stable_turn = |id: &str| {
        let segment = Some(SegmentRef {
            segment_id: Some(id.into()),
            update_kind: TranscriptUpdateKind::Append,
        });
        [
            Observed::Event(StreamEvent::Transcript {
                text: "x".into(),
                stability: TranscriptStability::Committed,
                segment: segment.clone(),
            }),
            Observed::Event(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                segment,
            })),
        ]
    };
    // A supported dialect: an ordinary Turn, then the timeout that answers
    // the Finalize, is within the contract but not a normal fixture.
    let mut timed_out = vec![Observed::Finalize];
    timed_out.extend(stable_turn("final0"));
    timed_out.extend([
        finalize_error(ErrorCode::Timeout),
        Observed::Audio,
        closed(),
    ]);
    assert!(contract_breaches(true, &timed_out).is_empty());
    assert!(normal_fixture_breaches(&timed_out)[0].contains("under a normal fixture"));
    // An ordinary Turn, then the segment-less Turn that answers the Finalize.
    let mut answered = vec![Observed::Finalize];
    answered.extend(stable_turn("final0"));
    answered.extend([bare_turn(), Observed::Audio, closed()]);
    assert!(contract_breaches(true, &answered).is_empty());
    assert!(normal_fixture_breaches(&answered).is_empty());
}

/// The assertion set itself catches the breaches it names.
#[test]
fn contract_breaches_are_detected() {
    let segment = |id: &str, update_kind| {
        Some(SegmentRef {
            segment_id: Some(id.into()),
            update_kind,
        })
    };
    let transcript = |stability, segment| {
        Observed::Event(StreamEvent::Transcript {
            text: "x".into(),
            stability,
            segment,
        })
    };
    let turn = |segment| {
        Observed::Event(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
            segment,
        }))
    };
    let observed = vec![
        transcript(
            TranscriptStability::Provisional,
            segment("a", TranscriptUpdateKind::Append),
        ),
        transcript(
            TranscriptStability::Committed,
            segment("b", TranscriptUpdateKind::Snapshot),
        ),
        transcript(
            TranscriptStability::Provisional,
            segment("b", TranscriptUpdateKind::Snapshot),
        ),
        turn(segment("c", TranscriptUpdateKind::Snapshot)),
        turn(segment("b", TranscriptUpdateKind::Snapshot)),
        turn(segment("b", TranscriptUpdateKind::Snapshot)),
        turn(None),
        transcript(
            TranscriptStability::Committed,
            segment("d", TranscriptUpdateKind::Append),
        ),
        Observed::Finalize,
    ];
    let breaches = contract_breaches(true, &observed);
    for needle in [
        "Draft must be a Snapshot",
        "Draft on b after its Stable",
        "Turn points at c",
        "second Turn for b",
        "segment-less Turn",
        "Stable d was never ended by a Turn",
        "got neither a Turn nor a finalize error",
        "no audio after the last Finalize",
        "did not end with SessionClosed",
    ] {
        assert!(
            breaches.iter().any(|b| b.contains(needle)),
            "missing {needle:?} in {breaches:#?}"
        );
    }
}
