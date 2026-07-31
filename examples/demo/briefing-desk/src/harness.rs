#![allow(dead_code)]
//! Centralized editable harness surfaces for Briefing Desk.
//!
//! Candidate experiments may only change the prompt/description text defined
//! here. Tool schemas, execute bodies, runtime config, and fixtures are not
//! candidate surfaces.

/// Stable identifier for one editable surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceId {
    MainSystemPrompt,
    ReviewerSystemPrompt,
    ReviewReportToolDescription,
    SearchFixturesToolDescription,
    ReadFixtureToolDescription,
    WriteReportToolDescription,
    TranscribeAudioToolDescription,
    DescribeImageToolDescription,
    SynthesizeBriefToolDescription,
}

#[allow(dead_code)]
impl SurfaceId {
    pub const ALL: [SurfaceId; 9] = [
        SurfaceId::MainSystemPrompt,
        SurfaceId::ReviewerSystemPrompt,
        SurfaceId::ReviewReportToolDescription,
        SurfaceId::SearchFixturesToolDescription,
        SurfaceId::ReadFixtureToolDescription,
        SurfaceId::WriteReportToolDescription,
        SurfaceId::TranscribeAudioToolDescription,
        SurfaceId::DescribeImageToolDescription,
        SurfaceId::SynthesizeBriefToolDescription,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SurfaceId::MainSystemPrompt => "main.system_prompt",
            SurfaceId::ReviewerSystemPrompt => "reviewer.system_prompt",
            SurfaceId::ReviewReportToolDescription => "tool.review_report.description",
            SurfaceId::SearchFixturesToolDescription => "tool.search_fixtures.description",
            SurfaceId::ReadFixtureToolDescription => "tool.read_fixture.description",
            SurfaceId::WriteReportToolDescription => "tool.write_report.description",
            SurfaceId::TranscribeAudioToolDescription => "tool.transcribe_audio.description",
            SurfaceId::DescribeImageToolDescription => "tool.describe_image.description",
            SurfaceId::SynthesizeBriefToolDescription => "tool.synthesize_brief.description",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == id)
    }
}

/// Main agent system prompt used by `run`.
pub const MAIN_SYSTEM_PROMPT: &str =
    "You are Briefing Desk, a research-brief assistant. Search the materials, read the \
most relevant one, transcribe any audio source and describe any image source if \
those tools are available, have review_report check your draft, then call \
write_report. If synthesize_brief is available, call it last.";

/// Reviewer sub-agent system prompt.
pub const REVIEWER_SYSTEM_PROMPT: &str =
    "You are a report reviewer. Check the draft for accuracy against the corpus.";

/// `review_report` tool description.
pub const REVIEW_REPORT_TOOL_DESCRIPTION: &str =
    "Reviews a draft report before it is finalized. Call this before write_report.";

/// `search_fixtures` tool description.
pub const SEARCH_FIXTURES_TOOL_DESCRIPTION: &str =
    "Search local research materials for a query; returns ranked paths and snippets.";

/// `read_fixture` tool description.
pub const READ_FIXTURE_TOOL_DESCRIPTION: &str =
    "Read the full contents of a fixture file discovered by search_fixtures.";

/// `write_report` tool description.
pub const WRITE_REPORT_TOOL_DESCRIPTION: &str =
    "Write the final Markdown research brief to the configured output path. Requires approval.";

/// `transcribe_audio` tool description.
pub const TRANSCRIBE_AUDIO_TOOL_DESCRIPTION: &str =
    "Transcribe a recorded audio source from the materials corpus via ASR.";

/// `describe_image` tool description.
pub const DESCRIBE_IMAGE_TOOL_DESCRIPTION: &str =
    "Describe an image source from the materials corpus using a real vision model call.";

/// `synthesize_brief` tool description.
pub const SYNTHESIZE_BRIEF_TOOL_DESCRIPTION: &str =
    "Synthesize an audio version of the final brief via TTS. Requires approval.";

/// Return every editable surface as `(surface_id, text)` sorted by id string.
pub fn all_surfaces() -> Vec<(&'static str, &'static str)> {
    SurfaceId::ALL
        .into_iter()
        .map(|id| (id.as_str(), text_for(id)))
        .collect()
}

/// Look up the current text for a surface id.
pub fn text_for(id: SurfaceId) -> &'static str {
    match id {
        SurfaceId::MainSystemPrompt => MAIN_SYSTEM_PROMPT,
        SurfaceId::ReviewerSystemPrompt => REVIEWER_SYSTEM_PROMPT,
        SurfaceId::ReviewReportToolDescription => REVIEW_REPORT_TOOL_DESCRIPTION,
        SurfaceId::SearchFixturesToolDescription => SEARCH_FIXTURES_TOOL_DESCRIPTION,
        SurfaceId::ReadFixtureToolDescription => READ_FIXTURE_TOOL_DESCRIPTION,
        SurfaceId::WriteReportToolDescription => WRITE_REPORT_TOOL_DESCRIPTION,
        SurfaceId::TranscribeAudioToolDescription => TRANSCRIBE_AUDIO_TOOL_DESCRIPTION,
        SurfaceId::DescribeImageToolDescription => DESCRIBE_IMAGE_TOOL_DESCRIPTION,
        SurfaceId::SynthesizeBriefToolDescription => SYNTHESIZE_BRIEF_TOOL_DESCRIPTION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_ids_are_unique_and_stable() {
        let mut ids: Vec<_> = SurfaceId::ALL.iter().map(|s| s.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SurfaceId::ALL.len());
        assert_eq!(all_surfaces().len(), SurfaceId::ALL.len());
    }

    #[test]
    fn every_surface_has_nonempty_text() {
        for id in SurfaceId::ALL {
            assert!(!text_for(id).is_empty(), "{}", id.as_str());
        }
    }
}
