//! LRC generation engine.
//!
//! Priority chain:
//! 1. Provider-supplied LRC (stored directly, no processing needed)
//! 2. Text-based alignment (parses lyrics sections, distributes by weight)
//! 3. Future: forced alignment via whisper-rs

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

/// Split lyrics into sections by `[Label]` markers.
fn parse_sections(raw: &str) -> Vec<Section> {
    let parts: Vec<&str> = raw.split(&['[', ']']).collect();
    let mut sections: Vec<Section> = Vec::with_capacity(parts.len() / 2);
    let mut i = 0;
    while i + 1 < parts.len() {
        let label = parts[i].trim().to_string();
        let content = parts.get(i + 1).map(|s| s.trim()).unwrap_or("");
        if !label.is_empty() && !content.is_empty() {
            let lines: Vec<String> = content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if !lines.is_empty() {
                // Merge consecutive choruses
                if let Some(last) = sections.last_mut() {
                    if last.label.eq_ignore_ascii_case(&label)
                        && label.to_lowercase().contains("chorus")
                    {
                        last.lines.extend(lines);
                        i += 2;
                        continue;
                    }
                }
                sections.push(Section { label, lines });
            }
        }
        i += 2;
    }
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
}
