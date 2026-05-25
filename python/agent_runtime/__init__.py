"""Orchest Agent Runtime - Python SDK for building AI agents."""

from typing import Any, TypeAlias

from .agent_runtime_py import Agent
from .exceptions import (
    AgentError,
    ApprovalDeniedError,
    BudgetExceededError,
    ModelError,
    SkillError,
    ToolError,
)

JsonValue: TypeAlias = Any
JsonSchema: TypeAlias = dict[str, Any]
BudgetOptions: TypeAlias = dict[str, Any]
RequestOptions: TypeAlias = dict[str, Any]
TokenUsage: TypeAlias = dict[str, int | dict[str, int]]
ToolCall: TypeAlias = dict[str, Any]
ToolRegistration: TypeAlias = dict[str, Any]
RuntimeEvent: TypeAlias = dict[str, Any]

__all__ = [
    "Agent",
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
    "TokenUsage",
    "ToolCall",
    "ToolError",
    "ToolRegistration",
]
