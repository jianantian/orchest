"""Pull-based speech synthesis for the Python SDK."""

import json
from collections.abc import AsyncIterator
from types import TracebackType
from typing import Any, Literal, NotRequired, Self, TypeAlias, TypedDict, Unpack, cast

from .orchest_py import _NativeTtsStream, _open_tts_stream, _synthesize

TtsAudioFormat: TypeAlias = Literal[
    "pcm", "pcm16_le", "wav", "wav_pcm16_le", "opus", "ogg_opus", "mp3",
    "m4a", "aac", "ogg", "flac",
]


class TtsOptions(TypedDict):
    provider: str
    voice: str
    format: TtsAudioFormat
    model: NotRequired[str]
    api_key: NotRequired[str]
    api_key_env: NotRequired[str]
    api_url: NotRequired[str]
    provider_options: NotRequired[dict[str, Any]]
    options: NotRequired[dict[str, Any]]


class VoiceQuantity(TypedDict):
    value: int
    source: Literal["provider_reported", "provider_billed", "input_estimate"]


class VoiceRate(TypedDict):
    currency: str
    unit: Literal["audio_second", "billed_character"]
    price_per_unit: float
    scope: str
    source_url: str


class VoiceCostEstimate(TypedDict):
    amount: float
    rate: VoiceRate


class VoiceUsage(TypedDict):
    provider: str
    model: str
    outcome: Literal["completed", "cancelled", "failed"]
    audio_duration_ms: VoiceQuantity | None
    input_characters: int | None
    billed_characters: int | None
    cost_estimate: VoiceCostEstimate | None


class TtsResult(TypedDict):
    audio: bytes
    format: TtsAudioFormat
    usage: VoiceUsage | None


class TtsAudioEvent(TypedDict):
    kind: Literal["audio"]
    audio: bytes
    format: TtsAudioFormat
    sequence: int
    context_id: int | None
    flush_id: int | None


class TtsFlushEvent(TypedDict):
    kind: Literal["flush_accepted", "flushed"]
    context_id: int
    flush_id: int


class TtsContextEvent(TypedDict):
    kind: Literal["cancelled", "context_ended"]
    context_id: int


class TtsEndEvent(TypedDict):
    kind: Literal["done", "session_closed"]


class TtsErrorEvent(TypedDict):
    kind: Literal["error"]
    code: str
    message: str
    fatal: Literal[False]


TtsStreamEvent: TypeAlias = (
    TtsAudioEvent | TtsFlushEvent | TtsContextEvent | TtsEndEvent | TtsErrorEvent
)

_WIRE_NAMES = {
    "api_key": "apiKey",
    "api_key_env": "apiKeyEnv",
    "api_url": "apiUrl",
    "provider_options": "providerOptions",
}


def _encode(options: TtsOptions) -> str:
    return json.dumps({_WIRE_NAMES.get(key, key): value for key, value in options.items()})


async def synthesize(text: str, **options: Unpack[TtsOptions]) -> TtsResult:
    """Synthesize one complete text into native bytes and usage metadata."""
    return cast(TtsResult, await _synthesize(text, _encode(options)))


class TtsStream:
    """A pull-based synthesis stream. Use an async context for early cleanup."""

    def __init__(self, native: _NativeTtsStream) -> None:
        self._native = native

    async def __aenter__(self) -> Self:
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        await self.abort()

    async def __aiter__(self) -> AsyncIterator[TtsStreamEvent]:
        try:
            while (event := await self._native.next_event()) is not None:
                yield cast(TtsStreamEvent, event)
        finally:
            await self.abort()

    async def usage(self) -> VoiceUsage | None:
        """Read the settlement after draining the stream."""
        return cast(VoiceUsage | None, await self._native.usage())

    async def abort(self) -> None:
        """Release the stream immediately without waiting for more audio."""
        await self._native.abort()


class TtsDuplexStream(TtsStream):
    """Submit text and controls while a separate task consumes audio."""

    async def append(self, text: str) -> None:
        await self._native.command("append", text)

    async def flush(self) -> None:
        """Queue a Flush. The matching completion arrives in the event stream."""
        await self._native.command("flush")

    async def cancel(self) -> None:
        """Queue cancellation of the current synthesis context."""
        await self._native.command("cancel")

    async def close(self) -> None:
        """Close input and let the remaining audio drain. Safe to call twice."""
        await self._native.command("close")


async def stream_synthesize(text: str, **options: Unpack[TtsOptions]) -> TtsStream:
    """Start streaming audio for a complete text."""
    return TtsStream(await _open_tts_stream(_encode(options), text))


async def start_tts_duplex(**options: Unpack[TtsOptions]) -> TtsDuplexStream:
    """Open a duplex synthesis session with an explicit voice and format."""
    return TtsDuplexStream(await _open_tts_stream(_encode(options), None))
