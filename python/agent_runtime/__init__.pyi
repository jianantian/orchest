from collections.abc import Callable
from typing import Any

class Agent:
    def __init__(
        self,
        model: str,
        system_prompt: str,
        skills_dir: str | None = None,
        budget: dict[str, Any] | None = None,
        api_url: str | None = None,
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
    def run_sync(self, input: str) -> list[dict[str, Any]]: ...
    def run(self, input: str) -> list[dict[str, Any]]: ...
    def respond_approval(self, run_id_str: str, approved: bool) -> None: ...

__all__: list[str]
