"""Anthropic provider example: tool calling with Claude.

Falls back to a local mock provider when ANTHROPIC_API_KEY is not set,
so this example always runs out of the box.

Requires (optional): ANTHROPIC_API_KEY
"""

from __future__ import annotations

import os
import sys
import threading
from pathlib import Path
from typing import Any

from orchest import Agent

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "support"))
from mock_anthropic_provider import serve


def configure_provider() -> str | None:
    """Return api_url: None for real Anthropic, or a local mock URL."""
    if os.environ.get("ANTHROPIC_API_KEY"):
        return os.environ.get("ANTHROPIC_API_URL")

    os.environ["ANTHROPIC_API_KEY"] = "local-demo-key"
    server = serve(8789)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return "http://127.0.0.1:8789/v1/messages"


agent = Agent(
    model="anthropic/claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant. Answer concisely.",
    api_key_env="ANTHROPIC_API_KEY",
    api_url=configure_provider(),
    request_options={"thinking": "off", "max_tokens": 512},
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
