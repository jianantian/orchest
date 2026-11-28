"""Orchest Agent Runtime - Python SDK for building AI agents."""

import asyncio
import inspect
from collections.abc import Callable
from typing import Any, Literal, NotRequired, TypeAlias, TypedDict

from .orchest_py import (
    Agent,
    _NativeAsrStream,
    _NativeRun,
    _start_asr_stream,
    complete,
    decide,
    transcribe,
)
from .exceptions import (
    AgentError,
    ApprovalDeniedError,
    BudgetExceededError,
    ModelError,
    SkillError,
    ToolError,
)

JsonValue: TypeAlias = dict[str, Any] | list[Any] | str | int | float | bool | None
JsonSchema: TypeAlias = dict[str, Any]
BudgetOptions: TypeAlias = dict[str, Any]
RequestOptions: TypeAlias = dict[str, Any]
TokenUsage: TypeAlias = dict[str, int | dict[str, int]]
ToolCall: TypeAlias = dict[str, Any]
ToolRegistration: TypeAlias = dict[str, Any]
RuntimeEvent: TypeAlias = dict[str, Any]

CancelReason: TypeAlias = Literal["user", "timeout", "upstream", "host_disconnected", "watcher"]
ApprovalScope: TypeAlias = Literal["once", "session"]
Enforcement: TypeAlias = Literal["none", "soft", "hard"]
ToolEffect: TypeAlias = Literal[
    "read", "write_workspace", "write_outside", "network", "credential", "destructive", "process"
]


class ApproveRequest(TypedDict):
    type: Literal["approve"]


class DenyRequest(TypedDict):
    type: Literal["deny"]
    reason: NotRequired[str | None]


class AnswerRequest(TypedDict):
    type: Literal["answer"]
    payload: JsonValue


class RequestResponse(TypedDict):
    decision: Literal["approve", "deny"] | ApproveRequest | DenyRequest | AnswerRequest
    scope: NotRequired[ApprovalScope]
    session_rules: NotRequired[JsonValue]


class EnforcementFacts(TypedDict):
    backend: str | None
    isolation: Enforcement
    permission_gate: Enforcement
    code_execution: Enforcement | None
    read_only: Enforcement


class Capabilities(TypedDict):
    resume: bool
    preemptive_cancel: bool
    approval_scopes: list[ApprovalScope]
    builtin_request_kinds: list[str]
    host_request_kinds: bool
    parallel_tools: bool
    delegation: bool
    effects: list[ToolEffect]
    enforcement: EnforcementFacts
    lossless_events: bool


class Run:
    """An active agent run and its single, pull-based event iterator."""

    def __init__(self, native: _NativeRun) -> None:
        self._native = native
        self._read_lock = asyncio.Lock()
        self._pending: asyncio.Future[RuntimeEvent | None] | None = None

    def __aiter__(self) -> "Run":
        return self

    async def __anext__(self) -> RuntimeEvent:
        async with self._read_lock:
            if self._pending is None:
                self._pending = self._native.next_event()
            try:
                event = await asyncio.shield(self._pending)
            except asyncio.CancelledError:
                # A cancelled host task can retry without discarding the event.
                raise
            except Exception:
                self._pending = None
                raise
            self._pending = None
            if event is None:
                raise StopAsyncIteration
            return event

    async def __aenter__(self) -> "Run":
        return self

    async def __aexit__(self, *_: object) -> None:
        await self.aclose()

    def cancel(self, reason: CancelReason | None = None, message: str | None = None) -> bool:
        """Request cancellation while keeping the terminal event readable."""
        return self._native.cancel(reason, message)

    async def respond(self, request_id: str, payload: RequestResponse) -> None:
        """Answer a pending request on this run."""
        await self._native.respond(request_id, payload)

    def capabilities(self) -> Capabilities:
        """Report the capabilities captured when this run started."""
        return self._native.capabilities()

    async def wait(self) -> RuntimeEvent:
        """Return the final root-run event after the iterator reaches EOF."""
        return await self._native.wait()

    async def aclose(self) -> None:
        """Release the event receiver and wait for active work to finish."""
        await self._native.close()
        async with self._read_lock:
            if self._pending is not None:
                try:
                    await asyncio.shield(self._pending)
                finally:
                    self._pending = None


DecisionDescription: TypeAlias = str | list[JsonValue] | dict[str, JsonValue]
BooleanCriteria = TypedDict(
    "BooleanCriteria",
    {"true": DecisionDescription, "false": DecisionDescription},
)


class BooleanQuestion(TypedDict):
    type: Literal["boolean"]
    instructions: DecisionDescription
    criteria: NotRequired[BooleanCriteria]


class ChoiceQuestion(TypedDict):
    type: Literal["choice"]
    instructions: DecisionDescription
    criteria: dict[str, DecisionDescription | None]


class ScoreQuestion(TypedDict):
    type: Literal["score"]
    instructions: DecisionDescription
    criteria: list[DecisionDescription]


DecisionQuestion: TypeAlias = BooleanQuestion | ChoiceQuestion | ScoreQuestion


class BooleanAnswer(TypedDict):
    type: Literal["boolean"]
    probability: float


class ChoiceAnswer(TypedDict):
    type: Literal["choice"]
    choice: str
    probabilities: NotRequired[dict[str, float]]
    confidence: NotRequired[float]


class ScoreAnswer(TypedDict):
    type: Literal["score"]
    score: float
    legend: NotRequired[dict[str, JsonValue]]
    probabilities: NotRequired[dict[str, float]]
    confidence: NotRequired[float]


DecisionAnswer: TypeAlias = BooleanAnswer | ChoiceAnswer | ScoreAnswer


class DecisionUsage(TypedDict):
    input_tokens: int
    output_tokens: int
    cost_usd: NotRequired[float]


class DecisionResponse(TypedDict):
    model: str
    answers: dict[str, DecisionAnswer]
    usage: NotRequired[DecisionUsage]
    id: NotRequired[str]
    provider: NotRequired[str]


class AsrContextMessage(TypedDict):
    role: Literal["user", "assistant"]
    text: str


class AsrDraftEvent(TypedDict):
    """Provisional text; the next draft of the same segment replaces it."""

    kind: Literal["draft"]
    text: str
    segment_id: str | None


class AsrStableEvent(TypedDict):
    """Confirmed text. ``update`` says whether it appends to the segment or
    replaces the segment's confirmed text."""

    kind: Literal["stable"]
    text: str
    segment_id: str | None
    update: Literal["append", "snapshot"]


class AsrTurnEvent(TypedDict):
    """One round of speech ended; ``segment_id`` names the stable it ends."""

    kind: Literal["turn"]
    segment_id: str | None


class AsrSessionStartedEvent(TypedDict):
    kind: Literal["session_started"]
    session_id: str | None


class AsrRouteSelectedEvent(TypedDict):
    kind: Literal["route_selected"]
    provider: str
    model: str


class AsrSessionClosedEvent(TypedDict):
    kind: Literal["session_closed"]


class AsrErrorEvent(TypedDict):
    """A provider error. Fatal errors also end the stream and are raised by
    :meth:`AsrStream.wait`."""

    kind: Literal["error"]
    code: str | None
    message: str
    fatal: bool
    # "finalize" when the error answers finalize(); such an error is non-fatal
    operation: str | None
    # the upstream's own error detail ({"code", "message", ...}), when it sent one
    upstream: dict[str, object] | None


class AsrExtrasEvent(TypedDict):
    """Provider-specific detail outside the draft / stable / turn model."""

    kind: Literal["extras"]
    data: JsonValue


AsrStreamEvent: TypeAlias = (
    AsrDraftEvent
    | AsrStableEvent
    | AsrTurnEvent
    | AsrSessionStartedEvent
    | AsrRouteSelectedEvent
    | AsrSessionClosedEvent
    | AsrErrorEvent
    | AsrExtrasEvent
)


class AsrStream:
    """A live ASR session returned by :func:`start_asr_stream`."""

    def __init__(
        self,
        native: _NativeAsrStream,
        callback_error: list[BaseException],
    ) -> None:
        self._native = native
        self._callback_error = callback_error

    async def send_audio(self, audio: bytes) -> None:
        """Send one audio chunk, waiting for input-channel capacity."""
        await self._native.send_audio(audio)

    async def finalize(self) -> None:
        """Ask the provider to finalize what it has heard so far, keeping the
        session open. Returning only means the request was submitted: the
        provider answers through ``on_event`` with a ``turn`` (after any
        ``stable``), or with a non-fatal ``error`` whose ``operation`` is
        ``"finalize"``, after which the stream stays usable. Raises
        :class:`ModelError` with code ``unsupported_option`` when the model
        does not support finalize (nothing is sent), and raises when the
        session is already finishing or closed."""
        await self._native.finalize()

    def finish(self) -> None:
        """Close the input side. Calling this more than once is safe."""
        self._native.finish()

    async def wait(self) -> None:
        """Wait for completion and re-raise the first callback exception."""
        native_error: BaseException | None = None
        try:
            await self._native.wait()
        except BaseException as error:
            native_error = error
        if self._callback_error:
            raise self._callback_error[0]
        if native_error is not None:
            raise native_error


async def start_asr_stream(
    *,
    format: str,
    sample_rate: int,
    on_event: Callable[[AsrStreamEvent], None],
    language: str | None = None,
    provider: str | None = None,
    model: str | None = None,
    api_key: str | None = None,
    api_key_env: str | None = None,
    api_url: str | None = None,
    context: list[AsrContextMessage] | None = None,
    options: dict[str, Any] | None = None,
) -> AsrStream:
    """Start a realtime ASR session once the provider has accepted it.

    ``provider`` is a provider name such as ``"deepgram"`` and ``model`` its
    model id; with neither, the default Aliyun streaming model is used, and
    with only ``provider``, that provider's streaming model."""
    callback_error: list[BaseException] = []
    native_holder: list[_NativeAsrStream] = []

    def guarded_on_event(event: AsrStreamEvent) -> None:
        if callback_error:
            return
        try:
            result = on_event(event)
            if inspect.isawaitable(result):
                if inspect.iscoroutine(result):
                    result.close()
                raise TypeError("on_event must be synchronous and return None")
        except BaseException as error:
            callback_error.append(error)
            if native_holder:
                native_holder[0].finish()

    native = await _start_asr_stream(
        format,
        sample_rate,
        guarded_on_event,
        language,
        provider,
        model,
        api_key,
        api_key_env,
        api_url,
        context,
        options,
    )
    native_holder.append(native)
    if callback_error:
        native.finish()
    return AsrStream(native, callback_error)

__all__ = [
    "Agent",
    "Run",
    "AsrStream",
    "AsrStreamEvent",
    "AsrContextMessage",
    "AsrDraftEvent",
    "AsrStableEvent",
    "AsrTurnEvent",
    "AsrSessionStartedEvent",
    "AsrRouteSelectedEvent",
    "AsrSessionClosedEvent",
    "AsrErrorEvent",
    "AsrExtrasEvent",
    "complete",
    "decide",
    "BooleanCriteria",
    "BooleanAnswer",
    "BooleanQuestion",
    "ChoiceQuestion",
    "DecisionDescription",
    "DecisionAnswer",
    "DecisionQuestion",
    "DecisionResponse",
    "DecisionUsage",
    "AgentError",
    "ApprovalDeniedError",
    "BudgetOptions",
    "BudgetExceededError",
    "JsonSchema",
    "JsonValue",
    "ModelError",
    "RequestOptions",
    "RuntimeEvent",
    "CancelReason",
    "RequestResponse",
    "Capabilities",
    "ToolEffect",
    "ApprovalScope",
    "Enforcement",
    "EnforcementFacts",
    "ApproveRequest",
    "DenyRequest",
    "AnswerRequest",
    "SkillError",
    "ScoreQuestion",
    "ScoreAnswer",
    "ChoiceAnswer",
    "start_asr_stream",
    "TokenUsage",
    "ToolCall",
    "ToolError",
    "ToolRegistration",
    "transcribe",
]
