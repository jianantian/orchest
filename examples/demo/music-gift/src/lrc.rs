#![allow(dead_code)]

//! LRC generation engine.
//!
//! Priority chain:
//! 1. Provider forced-alignment via [`orchest_protocol::TimedText`] — rendered
//!    to LRC by [`timed_text_to_lrc`] (real per-line timing).
//! 2. Text-based alignment (parses lyrics sections, distributes by weight) —
//!    the estimate fallback when the provider gives no timed text.

use orchest_protocol::TimedText;

/// Render provider forced-aligned [`TimedText`] to LRC.
///
/// Each segment's real lyric lines — section headers and inline delivery tags
/// stripped by [`content_line`] — are emitted at that segment's `start`
/// (block-level alignment, exact enough for a scrolling view). Text is
/// provider-verbatim until here; cleanup is the consumer's job by design.
/// Returns `None` if nothing renders.
pub fn timed_text_to_lrc(tt: &TimedText) -> Option<String> {
    let mut out = String::new();
    // Strip `[..]` tags with a state machine that spans segments: word-level
    // alignment can split a header like `[Verse 1 — tender]` across two
    // segments (`[Verse 1 —`, `tender]`), so per-segment cleanup isn't enough.
    let mut in_tag = false;
    for seg in &tt.segments {
        let mut clean = String::new();
        for ch in seg.text.chars() {
            match ch {
                '[' => in_tag = true,
                ']' => in_tag = false,
                c if !in_tag => clean.push(c),
                _ => {}
            }
        }
        for line in clean.lines() {
            let line = line.trim();
            if !line.is_empty() {
                let mins = (seg.start as u64) / 60;
                let secs = seg.start % 60.0;
                out.push_str(&format!("[{:02}:{:05.2}]{}\n", mins, secs, line));
            }
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }
    (!out.is_empty()).then_some(out)
}

/// Generate LRC text from structured lyrics and audio duration.
///
/// Parses `[Verse]`, `[Chorus]`, `[Bridge]`, `[Outro]`, `[Intro]` section
/// markers, weights each section by its musical role, and distributes the
/// audio duration across each line within a section.
pub fn generate_lrc(lyrics: &str, duration_secs: f64) -> Option<String> {
    let sections = parse_sections(lyrics);
    if sections.is_empty() {
        return None;
    }

    let total_weight: f64 = sections.iter().map(|s| section_weight(&s.label) * s.lines.len() as f64).sum();
    if total_weight <= 0.0 {
        return None;
    }

    let mut lrc = String::new();
    let mut current_time = 0.0;
    let secs_per_weight = duration_secs / total_weight;

    for section in &sections {
        let weight = section_weight(&section.label);
        for line in &section.lines {
            let mins = (current_time as u64) / 60;
            let secs = current_time % 60.0;
            lrc.push_str(&format!("[{:02}:{:05.2}]{}\n", mins, secs, line));
            // Move time forward proportional to this section's weight
            current_time += secs_per_weight * weight;
        }
    }

    // Trim trailing newline
    if lrc.ends_with('\n') {
        lrc.pop();
    }
    Some(lrc)
}

struct Section {
    label: String,
    lines: Vec<String>,
}

const SECTION_KEYWORDS: &[&str] = &[
    "verse", "chorus", "bridge", "intro", "outro", "pre-chorus", "prechorus",
    "hook", "refrain", "interlude",
];

/// If a line is a whole-line section header like `[Chorus]` or
/// `[Verse 1 — tender]`, return its label. Inline delivery tags such as
/// `[Whispered] I love you` are content, not headers, and standalone accent
/// tags like `[Whispered]` carry no section keyword, so neither is matched.
fn section_label(line: &str) -> Option<String> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    if inner.contains('[') {
        return None; // more than one tag on the line — not a bare header
    }
    let lower = inner.to_lowercase();
    SECTION_KEYWORDS
        .iter()
        .any(|k| lower.contains(k))
        .then(|| inner.trim().to_string())
}

/// A lyric line, with any leading delivery tag (`[Whispered] …`) stripped.
/// Returns `None` for lines that are only a bracket tag (voice accents,
/// `[Instrumental]`, `[Guitar solo]`) — those carry no sung text.
fn content_line(line: &str) -> Option<String> {
    let mut s = line.trim();
    if s.starts_with('[') {
        if let Some(end) = s.find(']') {
            s = s[end + 1..].trim();
        }
    }
    (!s.is_empty()).then(|| s.to_string())
}

/// Split lyrics into sections by whole-line `[Label]` headers.
///
/// Parses line by line rather than splitting on every `[`/`]`, so inline voice
/// tags (`[Whispered]`, `[Belting]`) inside a lyric line don't get mistaken for
/// section boundaries — the previous split-based parser corrupted every song
/// that used them.
fn parse_sections(raw: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(label) = section_label(trimmed) {
            // Merge a repeated chorus into the one before it instead of opening
            // a new section, so its lines flow together.
            let merge_into_prev = sections.last().is_some_and(|last| {
                last.label.eq_ignore_ascii_case(&label) && label.to_lowercase().contains("chorus")
            });
            if !merge_into_prev {
                sections.push(Section { label, lines: Vec::new() });
            }
        } else if let Some(text) = content_line(trimmed) {
            if let Some(last) = sections.last_mut() {
                last.lines.push(text);
            }
        }
    }
    sections.retain(|s| !s.lines.is_empty());
    sections
}

/// Weight factor by section type. Verses carry the most lyrical density.
fn section_weight(label: &str) -> f64 {
    let lower = label.to_lowercase();
    if lower.contains("intro") {
        0.7
    } else if lower.contains("outro") {
        0.5
    } else if lower.contains("bridge") {
        0.9
    } else if lower.contains("pre-chorus") || lower.contains("prechorus") {
        1.1
    } else {
        // verse, chorus, or unknown → default 1.0
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_lrc_for_simple_lyrics() {
        let lyrics = "[Verse 1]\nline one\nline two\n[Chorus]\nhook one\nhook two";
        let lrc = generate_lrc(lyrics, 120.0).unwrap();
        assert!(lrc.starts_with("[00:00.00]line one"));
        assert!(lrc.contains("line two"));
        assert!(lrc.contains("hook one"));
    }

    #[test]
    fn empty_lyrics_returns_none() {
        assert!(generate_lrc("", 60.0).is_none());
    }

    #[test]
    fn merges_consecutive_choruses() {
        let lyrics = "[Chorus]\nhook one\n[Chorus]\nhook two";
        let lrc = generate_lrc(lyrics, 60.0).unwrap();
        let lines: Vec<_> = lrc.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    /// Provider forced-aligned timed text renders to LRC at real per-segment
    /// times, with section headers and inline delivery tags stripped.
    #[test]
    fn timed_text_renders_lrc_stripping_tags() {
        use orchest_protocol::{TimedSegment, TimedText};
        let tt = TimedText {
            segments: vec![
                TimedSegment {
                    text: "[Verse 1 — gentle]\n晨光爬上窗台\n".into(),
                    start: 11.011,
                    end: Some(16.676),
                },
                TimedSegment {
                    text: "[Whispered] 你还在睡".into(),
                    start: 16.835,
                    end: None,
                },
            ],
        };
        let lrc = timed_text_to_lrc(&tt).unwrap();
        let lines: Vec<_> = lrc.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("[00:11."), "line 0: {}", lines[0]);
        assert!(lines[0].ends_with("晨光爬上窗台"));
        assert!(lines[1].starts_with("[00:16."), "line 1: {}", lines[1]);
        assert!(lines[1].ends_with("你还在睡")); // inline [Whispered] stripped
        assert!(!lrc.contains("Verse") && !lrc.contains("Whispered"));
    }

    /// Word-level alignment can split a section header across two segments; the
    /// cross-segment state machine must still strip it whole.
    #[test]
    fn timed_text_strips_tag_split_across_segments() {
        use orchest_protocol::{TimedSegment, TimedText};
        let tt = TimedText {
            segments: vec![
                TimedSegment { text: "[Verse 1 —".into(), start: 8.94, end: None },
                TimedSegment { text: "tender]".into(), start: 9.06, end: None },
                TimedSegment { text: "月光洒在窗前".into(), start: 9.18, end: None },
            ],
        };
        let lrc = timed_text_to_lrc(&tt).unwrap();
        let lines: Vec<_> = lrc.lines().collect();
        assert_eq!(lines.len(), 1, "tag segments should render nothing: {lrc}");
        assert!(!lrc.contains("Verse") && !lrc.contains("tender"), "tag leaked: {lrc}");
        assert!(lines[0].ends_with("月光洒在窗前"));
        assert!(lines[0].starts_with("[00:09."));
    }

    /// Inline voice tags used to be mistaken for section boundaries and split
    /// lyric lines apart. Section headers with em-dash cues must still parse,
    /// standalone accents must be skipped, and leading inline tags stripped.
    #[test]
    fn tolerates_inline_voice_tags() {
        let lyrics = "[Verse 1 — tender]\n[Whispered] 那天风很轻\n你笑了\n[Chorus — soaring]\n跑吧 朵朵";
        let lrc = generate_lrc(lyrics, 90.0).unwrap();
        let lines: Vec<_> = lrc.lines().collect();
        // 3 lyric lines: the whispered line (tag stripped), "你笑了", "跑吧 朵朵".
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("那天风很轻"), "leading tag not stripped: {}", lines[0]);
        assert!(!lrc.contains("[Whispered]"));
        assert!(!lrc.contains("Verse 1"), "section header leaked into lyrics");
        assert!(lrc.contains("跑吧 朵朵"));
    }
}
