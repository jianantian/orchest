"""OpenRouter provider example: Claude via OpenRouter with tool calling.

Requires: OPENROUTER_API_KEY
"""

from __future__ import annotations

import os
from typing import Any

from orchest import Agent

agent = Agent(
    name="openrouter-assistant",
    model="openrouter/anthropic/claude-sonnet-4-6",
    system_prompt="You are a helpful assistant. Answer concisely.",
    api_key_env="OPENROUTER_API_KEY",
    request_options={"thinking": "minimal", "max_tokens": 512},
)


@agent.tool
def get_weather(city: str) -> dict[str, Any]:
    """Get the current weather for a city."""
    return {"city": city, "temperature": 22, "condition": "sunny"}


def _extract_text(delta: Any) -> str:
    if isinstance(delta, str):
        return ""
    if isinstance(delta, dict):
        text = delta.get("Text") or delta.get("text") or {}
        if isinstance(text, dict):
            return text.get("delta", "")
    return ""


if __name__ == "__main__":
    if not os.environ.get("OPENROUTER_API_KEY"):
        print("Set OPENROUTER_API_KEY first.")
        exit(1)

    events = agent.run("What's the weather in Tokyo? One sentence.")
    for event in events:
        etype = event.get("type", "unknown")

        if etype == "model_stream_chunk":
            print(_extract_text(event.get("delta")), end="", flush=True)

        elif etype == "tool_call_started":
            print(f"\n[Tool] {event.get('tool')}({event.get('input')})")

        elif etype == "tool_call_completed":
            print(f"[Result] {event.get('output')}")

        elif etype == "run_completed":
            print("\n\nDone.")

        elif etype == "run_failed":
            print(f"\n[Error] {event.get('error')}")
    print()
