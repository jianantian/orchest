"""Structured exceptions for the Orchest Python SDK."""


class AgentError(Exception):
    """Base exception for all agent runtime errors."""

    def __init__(self, message: str, code: str | None = None) -> None:
        super().__init__(message)
        self.code = code

    def __repr__(self) -> str:
        return f"{type(self).__name__}(message={str(self)!r}, code={self.code!r})"


class BudgetExceededError(AgentError):
    """Raised when the agent exceeds its configured budget."""


class ApprovalDeniedError(AgentError):
    """Raised when a required tool approval is denied."""


class ModelError(AgentError):
    """Raised when the LLM provider returns an error."""

    def __init__(
        self,
        message: str,
        code: str | None = None,
        *,
        provider: str | None = None,
        model: str | None = None,
        status: int | None = None,
        retry_after_secs: int | None = None,
        upstream: object | None = None,
        diagnostic_metadata: object | None = None,
    ) -> None:
        super().__init__(message, code)
        self.provider = provider
        self.model = model
        self.status = status
        self.retry_after_secs = retry_after_secs
        self.upstream = upstream
        self.diagnostic_metadata = diagnostic_metadata


class ToolError(AgentError):
    """Raised when a tool execution fails."""


class SkillError(AgentError):
    """Raised when skill loading or execution fails."""


def from_code(message: str, code: str | None) -> AgentError:
    """Map a runtime error code string to an exception subclass."""

    mapping: dict[str, type[AgentError]] = {
        "budget_exceeded": BudgetExceededError,
        "max_steps_reached": BudgetExceededError,
        "approval_denied": ApprovalDeniedError,
        "model_error": ModelError,
        "tool_error": ToolError,
        "skill_error": SkillError,
    }
    exception_type = mapping.get(code or "", AgentError)
    return exception_type(message, code)
