#!/usr/bin/env bash
# Deterministically generate fixtures/research/interview.wav.
#
# macOS-only convenience script: uses the built-in `say` TTS engine so the
# fixture contains real, intelligible speech (needed so a live ASR provider
# run can be checked against an expected transcript — see README.md). Not
# part of the build or CI; run once locally and commit the resulting WAV.
#
# Usage: ./gen_interview_audio.sh

set -euo pipefail

if ! command -v say >/dev/null 2>&1; then
  echo "error: this script requires macOS 'say' and is not portable to other platforms." >&2
  echo "if you are not on macOS, record or synthesize interview.wav by hand instead," >&2
  echo "matching the script below, and place it at ../research/interview.wav" >&2
  exit 1
fi

OUT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../research" && pwd)"
OUT_FILE="${OUT_DIR}/interview.wav"

# Two-voice mini-interview. The quotable line ("Honestly? My team would go
# straight back to spreadsheets ... nobody puts in a slide deck.") exists
# ONLY in this audio fixture — it is deliberately not transcribed anywhere
# in the text fixtures, so the agent must go through the ASR path to cite it.
SCRIPT="[[slnc 300]] What would happen if Loom disappeared tomorrow? [[slnc 400]] Honestly? My team would go straight back to spreadsheets and lose about two hours a day. That's the real return on investment nobody puts in a slide deck. [[slnc 200]]"

say -v Samantha -o "${OUT_FILE}" --data-format=LEI16@16000 "${SCRIPT}"

echo "wrote ${OUT_FILE}"
