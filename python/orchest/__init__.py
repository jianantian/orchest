"""Orchest Agent Runtime - Python SDK for building AI agents."""

import inspect
from collections.abc import Callable
from typing import Any, Literal, NotRequired, TypeAlias, TypedDict

from .orchest_py import (
    Agent,
    _NativeAsrStream,
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
AsrStreamEvent: TypeAlias = dict[str, Any]

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
    api_key: str | None = None,
    api_key_env: str | None = None,
    api_url: str | None = None,
    context: list[AsrContextMessage] | None = None,
    options: dict[str, Any] | None = None,
) -> AsrStream:
    """Start realtime ASR after the provider acknowledges ``task-started``."""
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
    "AsrStream",
    "AsrStreamEvent",
    "AsrContextMessage",
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
