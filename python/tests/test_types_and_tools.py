"""Tests for type aliases, tool registration, and event consumption helpers."""

from orchest import (
    Agent,
    AgentError,
    ApprovalDeniedError,
    BudgetExceededError,
    JsonValue,
    ModelError,
    SkillError,
    ToolError,
)
from orchest.exceptions import from_code


# --- Exception hierarchy ---


def test_exception_hierarchy() -> None:
    """All concrete exception classes inherit from AgentError."""
    for cls in (BudgetExceededError, ApprovalDeniedError, ModelError, ToolError, SkillError):
        exc = cls("test")
        assert isinstance(exc, AgentError)
        assert isinstance(exc, Exception)


def test_from_code_all_variants() -> None:
    mapping = {
        "budget_exceeded": BudgetExceededError,
        "max_steps_reached": BudgetExceededError,
        "approval_denied": ApprovalDeniedError,
        "model_error": ModelError,
        "tool_error": ToolError,
        "skill_error": SkillError,
    }
    for code, expected_cls in mapping.items():
        exc = from_code(f"msg for {code}", code)
        assert isinstance(exc, expected_cls), f"from_code({code!r}) should be {expected_cls.__name__}"
        assert exc.code == code


# --- Tool registration API ---


def test_agent_exposes_tool_decorator() -> None:
    """Agent.tool should be callable as a decorator."""
    assert callable(getattr(Agent, "tool", None))


def test_agent_exposes_register_tool() -> None:
    """Agent.register_tool should be available."""
    assert callable(getattr(Agent, "register_tool", None))


def test_agent_exposes_register_agent_tool() -> None:
    """Agent.register_agent_tool should be available for delegation."""
    assert callable(getattr(Agent, "register_agent_tool", None))


def test_agent_exposes_run_stream() -> None:
    """Agent.run_stream should be available for streaming execution."""
    assert callable(getattr(Agent, "run_stream", None))


def test_agent_exposes_respond_approval() -> None:
    """Agent.respond_approval should be available."""
    assert callable(getattr(Agent, "respond_approval", None))


# --- JsonValue type ---


def test_json_value_accepts_primitives() -> None:
    """JsonValue should accept all JSON-representable Python values."""
    values: list[JsonValue] = [None, True, 42, 3.14, "hello", [1, 2], {"a": 1}]
    # Just verify the type alias is usable at runtime (no TypeError)
    assert len(values) == 7
