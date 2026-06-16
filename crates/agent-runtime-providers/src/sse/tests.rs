use super::*;
use bytes::Bytes;
use futures_util::stream;

fn make_stream(chunks: Vec<&str>) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Unpin {
    stream::iter(
        chunks
            .into_iter()
            .map(|s| Ok(Bytes::from(s.to_string())))
            .collect::<Vec<_>>(),
    )
}

fn make_stream_owned(
    chunks: Vec<String>,
) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Unpin {
    stream::iter(
        chunks
            .into_iter()
            .map(|s| Ok(Bytes::from(s)))
            .collect::<Vec<_>>(),
    )
}

#[tokio::test]
async fn parse_buffered_chunks() {
    // Split an SSE event across two chunks
    let s = make_stream(vec![
        "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n",
        "\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n",
    ]);
    let (tx, mut rx) = mpsc::channel(16);
    let r = parse_openai_sse_stream(s, Some(&tx), None, None, Instant::now())
        .await
        .unwrap();
    let content = r.content;
    drop(tx);

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(matches!(&events[0], StreamEvent::Text { delta } if delta == "hel"));
    assert!(matches!(&events[1], StreamEvent::Text { delta } if delta == "lo"));
    assert!(matches!(&content[0], ContentBlock::Text(t) if t == "hello"));
}

#[tokio::test]
async fn parse_with_reasoning_field() {
    let sse = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\"}}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\n\
                   data: {\"choices\":[{\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3}}\n\n\
                   data: [DONE]\n\n";
    let s = make_stream(vec![sse]);
    let (tx, mut rx) = mpsc::channel(16);
    let SseParseResult {
        content,
        usage,
        stop_reason: stop,
        ..
    } = parse_openai_sse_stream(
        s,
        Some(&tx),
        Some("reasoning_content"),
        None,
        Instant::now(),
    )
    .await
    .unwrap();
    drop(tx);

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(matches!(events[0], StreamEvent::ThinkingStart));
    assert!(matches!(&events[1], StreamEvent::Thinking { delta } if delta == "think"));
    assert!(matches!(events[2], StreamEvent::ThinkingEnd { .. }));
    assert!(matches!(&events[3], StreamEvent::Text { delta } if delta == "answer"));

    assert_eq!(content.len(), 2);
    assert!(
        matches!(&content[0], ContentBlock::Thinking { text, .. } if text.as_deref() == Some("think"))
    );
    assert!(matches!(&content[1], ContentBlock::Text(t) if t == "answer"));
    assert_eq!(usage.input_tokens, 5);
    assert_eq!(stop, StopReason::EndTurn);
}

#[tokio::test]
async fn parse_without_reasoning_field() {
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
                   data: {\"choices\":[{\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n\
                   data: [DONE]\n\n";
    let s = make_stream(vec![sse]);
    let (tx, mut rx) = mpsc::channel(16);
    let r = parse_openai_sse_stream(s, Some(&tx), None, None, Instant::now())
        .await
        .unwrap();
    let content = r.content;
    drop(tx);

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(!events
        .iter()
        .any(|e| matches!(e, StreamEvent::ThinkingStart)));
    assert_eq!(content.len(), 1);
}

#[tokio::test]
async fn parse_tool_calls() {
    let chunk1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"echo","arguments":"{\"text\":"}}]}}]}"#;
    let chunk2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"hi\"}"}}]},"finish_reason":"tool_calls"}]}"#;
    let sse = format!("{chunk1}\n\n{chunk2}\n\ndata: [DONE]\n\n");
    let s = make_stream_owned(vec![sse]);
    let (tx, mut rx) = mpsc::channel(16);
    let SseParseResult {
        content,
        stop_reason: stop,
        ..
    } = parse_openai_sse_stream(s, Some(&tx), None, None, Instant::now())
        .await
        .unwrap();
    drop(tx);

    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }

    assert!(
        matches!(&events[0], StreamEvent::ToolUseStart { id, name } if id == "c1" && name == "echo")
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolUseArgsChunk { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolUseEnd { id } if id == "c1")));
    assert_eq!(stop, StopReason::ToolUse);
    assert!(matches!(&content[0], ContentBlock::ToolUse { name, .. } if name == "echo"));
}

#[tokio::test]
async fn parse_done_signal() {
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\ndata: [DONE]\n\n";
    let s = make_stream(vec![sse]);
    let result = parse_openai_sse_stream(s, None, None, None, Instant::now()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn parse_malformed_json() {
    let sse = "data: {NOT VALID}\n\ndata: [DONE]\n\n";
    let s = make_stream(vec![sse]);
    let err = parse_openai_sse_stream(s, None, None, None, Instant::now())
        .await
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("invalid_json"));
}

#[tokio::test]
async fn parse_stream_interrupted() {
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
    let s = make_stream(vec![sse]);
    let err = parse_openai_sse_stream(s, None, None, None, Instant::now())
        .await
        .unwrap_err();
    assert_eq!(err.code.as_deref(), Some("stream_interrupted"));
}

#[tokio::test]
async fn parse_missing_usage() {
    let sse =
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n\
                   data: [DONE]\n\n";
    let s = make_stream(vec![sse]);
    let r = parse_openai_sse_stream(s, None, None, None, Instant::now())
        .await
        .unwrap();
    let usage = r.usage;
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
}
