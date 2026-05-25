"""Basic example: register tools and run an agent."""

import os
import sys
import threading
from pathlib import Path

from agent_runtime import Agent

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "support"))
from mock_anthropic_provider import serve


def configure_demo_provider() -> str | None:
    if os.environ.get("ANTHROPIC_API_KEY"):
        return os.environ.get("ANTHROPIC_API_URL")

    os.environ["ANTHROPIC_API_KEY"] = "local-demo-key"
    server = serve(8787)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return "http://127.0.0.1:8787/v1/messages"


agent = Agent(
    model="anthropic/claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant with access to tools.",
    api_url=configure_demo_provider(),
)


@agent.tool
def get_weather(city: str) -> dict:
    """Get the current weather for a city."""
    return {"city": city, "temperature": 22, "condition": "sunny"}


@agent.tool
def get_time(timezone: str) -> dict:
    """Get the current time in a timezone."""
    return {"timezone": timezone, "time": "14:30:00"}


if __name__ == "__main__":
    events = agent.run("What's the weather in Tokyo and the current time in JST?")
    for event in events:
        event_type = event.get("type", "unknown")
        if event_type == "model_stream_chunk":
            delta = event.get("delta", {})
            if "Text" in delta or "text" in delta:
                text = delta.get("Text") or delta.get("text")
                print(text.get("delta", ""), end="", flush=True)
        elif event_type == "tool_call_started":
            print(f"\n[Tool Call] {event.get('tool', '?')}({event.get('input', {})})")
        elif event_type == "tool_call_completed":
            print(f"[Tool Result] {event.get('output', {})}")
        elif event_type == "run_completed":
            print(f"\n[Done] {event.get('output', '')}")
        elif event_type == "run_failed":
            print(f"\n[Error] {event.get('error', '')}")
    print()
