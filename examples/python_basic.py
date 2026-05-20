"""Basic example: register tools and run an agent."""

from agent_runtime_py import Agent


agent = Agent(
    model="claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant with access to tools.",
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
        if event_type == "ModelStreamChunk":
            delta = event.get("delta", {})
            if "Text" in delta:
                print(delta["Text"], end="", flush=True)
        elif event_type == "ToolCallStarted":
            print(f"\n[Tool Call] {event.get('tool', '?')}({event.get('input', {})})")
        elif event_type == "ToolCallCompleted":
            print(f"[Tool Result] {event.get('output', {})}")
        elif event_type == "RunCompleted":
            print(f"\n[Done] {event.get('output', '')}")
        elif event_type == "RunFailed":
            print(f"\n[Error] {event.get('error', '')}")
    print()
