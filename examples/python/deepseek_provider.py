"""DeepSeek V4 example: reasoning model with tool calling.

Shows DeepSeek's native thinking/reasoning blocks alongside tool use.
Uses deepseek-v4-pro for best reasoning quality.

Requires: DEEPSEEK_API_KEY
"""

from __future__ import annotations

import os
from typing import Any

from agent_runtime import Agent

agent = Agent(
    model="deepseek/deepseek-v4-pro",
    system_prompt="You are a concise reasoning assistant. Think step by step.",
    api_key_env="DEEPSEEK_API_KEY",
    request_options={"thinking": "high"},
)


@agent.tool
def get_weather(city: str) -> dict[str, Any]:
    """Get the current weather for a city."""
    return {"city": city, "temperature": 22, "condition": "sunny"}


def _extract_delta(delta: Any) -> tuple[str, str]:
    """Return (text, reasoning) from a stream delta. One will be empty."""
    text = ""
    reasoning = ""
    if isinstance(delta, str):
        return ("", "")
    if isinstance(delta, dict):
        t = delta.get("Text") or delta.get("text") or {}
        if isinstance(t, dict):
            text = t.get("delta", "")
        r = delta.get("Thinking") or delta.get("thinking") or {}
        if isinstance(r, dict):
            reasoning = r.get("delta", "")
    return (text, reasoning)


if __name__ == "__main__":
    if not os.environ.get("DEEPSEEK_API_KEY"):
        print("Set DEEPSEEK_API_KEY first.")
        exit(1)

    events = agent.run(
        "What's the weather in Tokyo and should I bring an umbrella? Explain your reasoning."
    )
    for event in events:
        etype = event.get("type", "unknown")

        if etype == "model_stream_chunk":
            text, reasoning = _extract_delta(event.get("delta"))
            if reasoning:
                print(f"\033[90m{reasoning}\033[0m", end="", flush=True)  # dim/gray
            if text:
                print(text, end="", flush=True)

        elif etype == "tool_call_started":
            print(f"\n\n[Tool] {event.get('tool')}({event.get('input')})")

        elif etype == "tool_call_completed":
            print(f"[Result] {event.get('output')}\n")

        elif etype == "run_completed":
            print("\n\nDone.")

        elif etype == "run_failed":
            print(f"\n[Error] {event.get('error')}")
    print()
