class AgentError(Exception):
    code: str | None
    def __init__(self, message: str, code: str | None = None) -> None: ...

class BudgetExceededError(AgentError): ...
class ApprovalDeniedError(AgentError): ...
class ModelError(AgentError):
    provider: str | None
    model: str | None
    status: int | None
    retry_after_secs: int | None
    upstream: object | None
    diagnostic_metadata: object | None
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
    ) -> None: ...
class ToolError(AgentError): ...
class SkillError(AgentError): ...

def from_code(message: str, code: str | None) -> AgentError: ...
