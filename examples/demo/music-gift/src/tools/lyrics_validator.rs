//! Lyrics validation — runs before Suno generation to catch structural issues.
//!
//! Returns warnings that are included in the generation params so Suno
//! has context to compensate.

/// Result of lyrics validation.
#[derive(Debug, Default)]
pub struct LyricsValidation {
    pub warnings: Vec<String>,
    pub section_count: usize,
    pub verse_count: usize,
    pub chorus_count: usize,
    pub word_count: usize,
    pub has_structure_tags: bool,
}

/// Validate lyrics and return warnings + stats.
///
/// Checks:
/// 1. Has [Verse] or [Chorus] section tags
/// 2. At least 2 chorus sections for song structure
/// 3. Word count (warns if under 100 or over 600)
/// 4. No twin verses (V1 first line == V2 first line)
pub fn validate_lyrics(lyrics: &str) -> LyricsValidation {
    let mut v = LyricsValidation::default();

    // Count section tags
    let lowered = lyrics.to_lowercase();
    v.section_count = lowered.matches("[verse").count() + lowered.matches("[chorus").count()
        + lowered.matches("[bridge").count() + lowered.matches("[pre-chorus").count()
        + lowered.matches("[intro").count() + lowered.matches("[outro").count()
        + lowered.matches("[instrumental").count();
    v.verse_count = lowered.matches("[verse").count();
    v.chorus_count = lowered.matches("[chorus").count();
    v.has_structure_tags = v.section_count > 0;

    // Word count
    v.word_count = lyrics.split_whitespace().count();

    if !v.has_structure_tags {
        v.warnings.push("No section tags ([Verse], [Chorus]) found. Suno may not structure the song well.".into());
    }
    if v.chorus_count < 2 {
        v.warnings.push("Less than 2 chorus sections. Songs need a repeating chorus for structure.".into());
    }
    if v.word_count < 100 {
        v.warnings.push(format!("Lyrics too short ({} words). Aim for 200-400 words for a full song.", v.word_count));
    }
    if v.word_count > 600 {
        v.warnings.push(format!("Lyrics too long ({} words). Suno may truncate or rush the delivery.", v.word_count));
    }

    // Check for twin verses (identical first lines)
    let lines: Vec<&str> = lyrics.lines().collect();
    let mut verse_starts: Vec<String> = vec![];
    let mut in_verse = false;
    for line in &lines {
        let trimmed = line.trim().to_lowercase();
        if trimmed.starts_with("[verse") {
            in_verse = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_verse = false;
            continue;
        }
        if in_verse && !trimmed.is_empty() {
            verse_starts.push(trimmed);
            in_verse = false; // Only capture first line of each verse
        }
    }
    for i in 0..verse_starts.len() {
        for j in (i + 1)..verse_starts.len() {
            if verse_starts[i] == verse_starts[j] {
                v.warnings.push(format!(
                    "Verse {} and Verse {} start identically ('{}'). Consider rewriting one for variety.",
                    i + 1, j + 1, verse_starts[i]
                ));
            }
        }
    }

    v
}
