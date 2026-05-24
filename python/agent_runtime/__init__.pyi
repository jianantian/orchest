from collections.abc import Callable
from typing import Any, Literal, NotRequired, TypeAlias, TypedDict


JsonValue: TypeAlias = (
    None
    | bool
    | int
    | float
    | str
    | list["JsonValue"]
    | dict[str, "JsonValue"]
)
JsonSchema: TypeAlias = dict[str, JsonValue]


class BudgetOptions(TypedDict, total=False):
    max_tokens: int
    max_tool_calls: int
    max_duration_secs: int
    max_cost_usd: float


class RequestOptions(TypedDict, total=False):
    thinking: Literal["off", "minimal", "low", "medium", "high", "xhigh", "max"]
    thinking_budget_tokens: int
    include_thinking: bool
    compatibility_policy: Literal["coerce", "strict"]
    max_tokens: int
    temperature: float
    top_p: float
    cache_policy: Literal["none", "auto", "long"]


class OptionAdjustment(TypedDict):
    option: str
    requested: JsonValue
    applied: JsonValue
    reason: str


class TokenUsage(TypedDict):
    input_tokens: int
    output_tokens: int
    reasoning_tokens: int
    cache_read_tokens: int
    cache_write_tokens: int
    details: dict[str, int]


class TextStreamEvent(TypedDict):
    Text: dict[str, str]


class ThinkingEndStreamEvent(TypedDict):
    ThinkingEnd: dict[str, JsonValue]


StreamEvent: TypeAlias = JsonValue


class ToolCall(TypedDict):
    id: str
    name: str
    input: JsonValue


class ToolRegistration(TypedDict):
    name: str
    description: str
    input_schema: JsonSchema
    requires_approval: NotRequired[bool]
    side_effect: NotRequired[bool]


class RunStartedEvent(TypedDict):
    type: Literal["run_started"]
    run_id: str
    run_depth: int
    child_run_id: str | None


class ModelCallStartedEvent(TypedDict):
    type: Literal["model_call_started"]
    step: int
    run_depth: int
    child_run_id: str | None


class ModelStreamChunkEvent(TypedDict):
    type: Literal["model_stream_chunk"]
    delta: StreamEvent
    run_depth: int
    child_run_id: str | None


class ModelCallCompletedEvent(TypedDict):
    type: Literal["model_call_completed"]
    tokens: TokenUsage
    option_adjustments: NotRequired[list[OptionAdjustment]]
    run_depth: int
    child_run_id: str | None


class ToolCallStartedEvent(TypedDict):
    type: Literal["tool_call_started"]
    tool: str
    source: JsonValue
    input: JsonValue
    run_depth: int
    child_run_id: str | None


class ToolCallUpdateEvent(TypedDict):
    type: Literal["tool_call_update"]
    tool: str
    tool_call_id: str
    partial: JsonValue
    run_depth: int
    child_run_id: str | None


class ToolCallCompletedEvent(TypedDict):
    type: Literal["tool_call_completed"]
    tool: str
    output: JsonValue
    duration: JsonValue
    run_depth: int
    child_run_id: str | None


class ToolCallFailedEvent(TypedDict):
    type: Literal["tool_call_failed"]
    tool: str
    error: str
    run_depth: int
    child_run_id: str | None


class AsyncToolStartedEvent(TypedDict):
    type: Literal["async_tool_started"]
    tool: str
    job_id: str
    run_depth: int
    child_run_id: str | None


class AsyncToolProgressEvent(TypedDict):
    type: Literal["async_tool_progress"]
    tool: str
    job_id: str
    status: JsonValue
    run_depth: int
    child_run_id: str | None


class AsyncToolCompletedEvent(TypedDict):
    type: Literal["async_tool_completed"]
    tool: str
    job_id: str
    output: JsonValue
    elapsed: JsonValue
    run_depth: int
    child_run_id: str | None


class SkillContentReadEvent(TypedDict):
    type: Literal["skill_content_read"]
    skill_name: str
    file: str
    tokens: int
    run_depth: int
    child_run_id: str | None


class ApprovalRequestedEvent(TypedDict):
    type: Literal["approval_requested"]
    tool_call: ToolCall
    run_depth: int
    child_run_id: str | None


class ApprovalGrantedEvent(TypedDict):
    type: Literal["approval_granted"]
    tool_call: ToolCall
    run_depth: int
    child_run_id: str | None


class ApprovalDeniedEvent(TypedDict):
    type: Literal["approval_denied"]
    tool_call: ToolCall
    run_depth: int
    child_run_id: str | None


class BudgetWarningEvent(TypedDict):
    type: Literal["budget_warning"]
    used: JsonValue
    limit: JsonValue
    run_depth: int
    child_run_id: str | None


class RuntimeWarningEvent(TypedDict):
    type: Literal["runtime_warning"]
    message: str
    run_depth: int
    child_run_id: str | None


class SkillDependencyErrorEvent(TypedDict):
    type: Literal["skill_dependency_error"]
    skill_name: str
    error: str
    run_depth: int
    child_run_id: str | None


class SkillMissingCapabilitiesEvent(TypedDict):
    type: Literal["skill_missing_capabilities"]
    skill_name: str
    run_depth: int
    child_run_id: str | None


class ContextCompactedEvent(TypedDict):
    type: Literal["context_compacted"]
    removed_messages: int
    summary_tokens: int
    run_depth: int
    child_run_id: str | None


class ChildRunEvent(TypedDict):
    type: Literal["child_run_event"]
    child_run_id: str
    run_depth: int
    event: "RuntimeEvent"


class SubAgentStartedEvent(TypedDict):
    type: Literal["sub_agent_started"]
    parent_run_id: str
    child_run_id: str
    config_summary: JsonValue
    run_depth: int


class SubAgentCompletedEvent(TypedDict):
    type: Literal["sub_agent_completed"]
    child_run_id: str
    output: JsonValue
    budget_used: JsonValue
    run_depth: int


class SubAgentFailedEvent(TypedDict):
    type: Literal["sub_agent_failed"]
    child_run_id: str
    error: str
    run_depth: int


class RunCompletedEvent(TypedDict):
    type: Literal["run_completed"]
    output: JsonValue
    run_depth: int
    child_run_id: str | None


class RunFailedEvent(TypedDict):
    type: Literal["run_failed"]
    error: str
    run_depth: int
    child_run_id: str | None


RuntimeEvent: TypeAlias = (
    RunStartedEvent
    | ModelCallStartedEvent
    | ModelStreamChunkEvent
    | ModelCallCompletedEvent
    | ToolCallStartedEvent
    | ToolCallUpdateEvent
    | ToolCallCompletedEvent
    | ToolCallFailedEvent
    | AsyncToolStartedEvent
    | AsyncToolProgressEvent
    | AsyncToolCompletedEvent
    | SkillContentReadEvent
    | ApprovalRequestedEvent
    | ApprovalGrantedEvent
    | ApprovalDeniedEvent
    | BudgetWarningEvent
    | RuntimeWarningEvent
    | SkillDependencyErrorEvent
    | SkillMissingCapabilitiesEvent
    | ContextCompactedEvent
    | ChildRunEvent
    | SubAgentStartedEvent
    | SubAgentCompletedEvent
    | SubAgentFailedEvent
    | RunCompletedEvent
    | RunFailedEvent
)

class Agent:
    def __init__(
        self,
        model: str,
        system_prompt: str,
        skills_dir: str | None = None,
        budget: BudgetOptions | None = None,
        api_url: str | None = None,
        api_key: str | None = None,
        api_key_env: str | None = None,
        max_tokens: int | None = None,
        request_options: RequestOptions | None = None,
    ) -> None: ...
    def set_api_url(self, api_url: str | None) -> None: ...
    def tool(
        self,
        func: Callable[..., Any] | None = None,
        requires_approval: bool = False,
        side_effect: bool = False,
    ) -> Callable[..., Any]: ...
    def register_tool(
        self,
        func: Callable[..., Any],
        requires_approval: bool = False,
        side_effect: bool = False,
    ) -> None: ...
    def register_agent_tool(
        self,
        name: str,
        description: str,
        agent: Agent,
        input_key: str | None = None,
    ) -> None: ...
    def register_write_file_tool(self, requires_approval: bool = True) -> None: ...
    def run_sync(self, input: str) -> list[RuntimeEvent]: ...
    def run(self, input: str) -> list[RuntimeEvent]: ...
    def respond_approval(self, run_id_str: str, approved: bool) -> None: ...

__all__: list[str]
