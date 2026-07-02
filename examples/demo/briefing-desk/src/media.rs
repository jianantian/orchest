//! Materials discovery and multimedia stand-ins.
//!
//! Real ASR transcription, vision image-reading and TTS synthesis land in
//! issue 005 via the `orchest-protocol` `Asr`/`Tts` gateways. This module only
//! classifies the materials directory and provides deterministic `--fake`
//! stand-ins so the CLI skeleton (issue 002) can exercise the full
//! transcribe -> read-image -> write -> synthesize pipeline offline.

use std::path::{Path, PathBuf};

use crate::app::DemoError;

#[derive(Debug, Default)]
pub struct Corpus {
    pub text: Vec<PathBuf>,
    pub images: Vec<PathBuf>,
    pub audio: Vec<PathBuf>,
}

/// Classify every file directly inside `dir` by extension. Unrecognized
/// extensions are ignored rather than treated as an error, since a materials
/// directory may reasonably contain notes, editor swap files, etc.
pub fn discover(dir: &Path) -> Result<Corpus, DemoError> {
    let mut corpus = Corpus::default();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))? {
        let path = entry
            .map_err(|e| format!("reading {}: {e}", dir.display()))?
            .path();
        if !path.is_file() {
            continue;
        }
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "md" | "txt" => corpus.text.push(path),
            "png" | "jpg" | "jpeg" => corpus.images.push(path),
            "wav" | "mp3" => corpus.audio.push(path),
            _ => {}
        }
    }
    corpus.text.sort();
    corpus.images.sort();
    corpus.audio.sort();
    Ok(corpus)
}

/// Deterministic ASR stand-in. Ignores actual audio content — issue 005 wires
/// a real `orchest_protocol::Asr` impl plus a matching fake for offline smoke.
pub fn fake_transcribe(path: &Path) -> String {
    format!(
        "[fake transcript placeholder for {} — real ASR lands in issue 005]",
        path.display()
    )
}

/// Deterministic vision stand-in. Ignores actual image content — issue 005
/// wires a real `ContentBlock::Image` vision-model path plus a fake for smoke.
pub fn fake_read_image(path: &Path) -> String {
    format!(
        "[fake image description placeholder for {} — real vision path lands in issue 005]",
        path.display()
    )
}

/// Deterministic TTS stand-in. Writes a small marker file, not real audio —
/// issue 005 wires a real `orchest_protocol::Tts` impl plus a matching fake.
pub fn fake_synthesize(brief: &str, out_path: &Path) -> Result<(), DemoError> {
    let marker = format!(
        "FAKE AUDIO PLACEHOLDER (real TTS lands in issue 005)\nbrief length: {} bytes\n",
        brief.len()
    );
    std::fs::write(out_path, marker)
        .map_err(|e| format!("writing {}: {e}", out_path.display()).into())
}
