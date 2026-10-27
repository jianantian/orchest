import pytest

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


def test_agent_requires_explicit_name() -> None:
    with pytest.raises(TypeError):
        Agent(model="anthropic/test", system_prompt="test")

    Agent(name="test-agent", model="anthropic/test", system_prompt="test")


def test_abort_without_active_run_returns_false() -> None:
    agent = Agent(name="test-agent", model="anthropic/test", system_prompt="test")
    assert agent.abort() is False
    with pytest.raises(ValueError):
        agent.abort(reason="bored")


def test_respond_requires_an_active_run_and_a_valid_payload() -> None:
    agent = Agent(name="test-agent", model="anthropic/test", system_prompt="test")
    with pytest.raises(ValueError):
        agent.respond("run/1", {"decision": "maybe"})
    with pytest.raises(RuntimeError):
        agent.respond("run/1", {"decision": "approve"})


def test_capabilities_report_the_runtime_contract() -> None:
    agent = Agent(name="test-agent", model="anthropic/test", system_prompt="test")
    caps = agent.capabilities()
    assert caps["preemptive_cancel"] is True
    assert caps["lossless_events"] is True
    assert "session" in caps["approval_scopes"]
