//! L0: a generic Server-Sent-Events line decoder (behind the `sse` feature).
//!
//! The shared core under the REST/SSE dialects (OpenAI-compat, Anthropic). It
//! parses the wire framing only — `event:` / `data:` fields accumulated until a
//! blank line dispatches an [`SseEvent`]; mapping a dispatched event to a
//! `StreamEvent` is the dialect's job, not this decoder's.

/// One dispatched SSE event: an optional `event:` name and the concatenated
/// `data:` payload (newline-joined, per the SSE spec).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

impl SseEvent {
    fn is_empty(&self) -> bool {
        self.event.is_none() && self.data.is_empty()
    }
}

/// Incremental SSE decoder. Feed it text chunks; it yields complete events as
/// blank-line boundaries are crossed.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: String,
    pending: SseEvent,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of decoded UTF-8 text; returns any events completed by it.
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        // Process complete lines (terminated by '\n'); keep any trailing partial.
        while let Some(nl) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=nl).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                // blank line → dispatch
                if !self.pending.is_empty() {
                    out.push(std::mem::take(&mut self.pending));
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix(':') {
                let _ = rest; // comment line — ignore
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (line, ""),
            };
            match field {
                "event" => self.pending.event = Some(value.to_string()),
                "data" => {
                    if !self.pending.data.is_empty() {
                        self.pending.data.push('\n');
                    }
                    self.pending.data.push_str(value);
                }
                _ => {} // id / retry / unknown — ignored by this core decoder
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event_and_multiline_data() {
        let mut d = SseDecoder::new();
        let events = d.push("event: message\ndata: hello\ndata: world\n\n");
        assert_eq!(
            events,
            vec![SseEvent {
                event: Some("message".into()),
                data: "hello\nworld".into(),
            }]
        );
    }

    #[test]
    fn handles_split_chunks_and_comments() {
        let mut d = SseDecoder::new();
        assert!(d.push(": keep-alive\ndata: par").is_empty());
        let events = d.push("tial\n\ndata: [DONE]\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "partial");
        assert_eq!(events[1].data, "[DONE]");
    }
}
