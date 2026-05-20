"""Async tool example: simulate a video generation task with progress."""

from agent_runtime_py import Agent


agent = Agent(
    model="claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant that can generate videos.",
    budget={"max_tool_calls": 5},
)


@agent.tool
def generate_video(prompt: str, duration_seconds: int) -> dict:
    """Generate a video from a text prompt. Returns immediately with a job ID."""
    import time
    time.sleep(0.1)  # Simulate API call
    return {
        "status": "started",
        "job_id": "vid_abc123",
        "estimated_seconds": duration_seconds * 2,
        "message": f"Started generating {duration_seconds}s video for: {prompt}",
    }


if __name__ == "__main__":
    events = agent.run("Generate a 5-second video of a cat playing piano")
    for event in events:
        event_type = event.get("type", "unknown")
        if event_type == "ModelStreamChunk":
            delta = event.get("delta", {})
            if "Text" in delta:
                print(delta["Text"], end="", flush=True)
        elif event_type == "ToolCallStarted":
            print(f"\n[Tool] {event.get('tool', '?')} started")
        elif event_type == "ToolCallCompleted":
            print(f"[Tool] Result: {event.get('output', {})}")
        elif event_type == "RunCompleted":
            print(f"\n[Done] {event.get('output', '')}")
        elif event_type == "RunFailed":
            print(f"\n[Error] {event.get('error', '')}")
    print()
