from orchest import Agent, AgentError, BudgetExceededError
from orchest.exceptions import from_code


def test_from_code_budget() -> None:
    exc = from_code("budget exceeded", "budget_exceeded")
    assert isinstance(exc, BudgetExceededError)
    assert exc.code == "budget_exceeded"


def test_from_code_unknown() -> None:
    exc = from_code("something went wrong", None)
    assert isinstance(exc, AgentError)
    assert exc.code is None


def test_agent_error_repr() -> None:
    exc = AgentError("test message", code="test_code")
    assert "test_code" in repr(exc)
    assert "test message" in repr(exc)


def test_agent_error_is_exception() -> None:
    exc = AgentError("test message", code="test_code")
    assert isinstance(exc, Exception)


def test_agent_exposes_run_sync() -> None:
    assert hasattr(Agent, "run_sync")
