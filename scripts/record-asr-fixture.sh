#!/usr/bin/env bash
# Re-record the ASR contract-test fixture of one streaming dialect from a live
# session (v1.2 Issue 003). The fixture keeps input sizes and frame kinds, never
# audio, keys or uplink payloads; downlink identifiers are redacted.
#
#   scripts/record-asr-fixture.sh <dialect> <audio.wav|.pcm> [extra recorder flags]
#
# dialect: aliyun | deepgram | elevenlabs | soniox | volcengine
# Keys: DASHSCOPE_API_KEY, DEEPGRAM_API_KEY, ELEVENLABS_API_KEY, SONIOX_API_KEY,
# VOLCENGINE_ASR_API_KEY (or pass --key-env). volcengine also needs
# --api-url wss://… and --option access_key=….
# The audio must be 16 kHz mono PCM s16le and say "hello world", pause, then
# "how are you" (the turns tests/asr_contract.rs expects). FINALIZE_AT_MS
# (default 2200) is when Finalize is sent; the script writes
# crates/orchest-provider-stream/tests/fixtures/asr/<dialect>/hello.jsonl.
set -euo pipefail

if [ $# -lt 2 ]; then
    sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi

dialect=$1
audio=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
shift 2
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/crates/orchest-provider-stream/tests/fixtures/asr/$dialect/hello.jsonl"
mkdir -p "$(dirname "$out")"

cd "$root"
cargo run -q -p orchest-provider-stream --features record-fixtures \
    --example record_asr_fixture -- \
    --dialect "$dialect" --audio "$audio" --out "$out" \
    --finalize-at-ms "${FINALIZE_AT_MS:-2200}" "$@"

if grep -Eo '"(api_key|authorization|token)": *"[^"]*"' "$out" | grep -vq '"redacted"'; then
    echo "error: $out looks like it contains a credential" >&2
    exit 1
fi
echo "now run: cargo test -p orchest-provider-stream --test asr_contract"
