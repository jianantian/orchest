"""Async tool example: simulate a video generation task with progress."""

import os
import threading

from agent_runtime import Agent

from mock_anthropic_provider import serve


def configure_demo_provider() -> str | None:
    if os.environ.get("ANTHROPIC_API_KEY"):
        return os.environ.get("ANTHROPIC_API_URL")

    os.environ["ANTHROPIC_API_KEY"] = "local-demo-key"
    server = serve(8788)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return "http://127.0.0.1:8788/v1/messages"


agent = Agent(
    model="claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant that can generate videos.",
    api_url=configure_demo_provider(),
    budget={"max_tool_calls": 5},
)


@agent.tool
def generate_video(prompt: str, duration_seconds: int) -> dict:
    """Generate a video from a text prompt. Returns immediately with a job ID."""
    polls = {"count": 0}

    def poll() -> dict:
        polls["count"] += 1
        if polls["count"] == 1:
            return {"status": "pending", "progress": 0.5, "message": "rendering"}
        return {
            "status": "completed",
            "result": {
                "video_url": "file:///tmp/demo-video.mp4",
                "prompt": prompt,
                "duration_seconds": duration_seconds,
            },
        }

    return {
        "async_job": {
            "job_id": "vid_abc123",
            "poll_interval_ms": 10,
            "poll": poll,
        }
    }


if __name__ == "__main__":
    events = agent.run("Generate a 5-second video of a cat playing piano")
    for event in events:
        event_type = event.get("type", "unknown")
        if event_type == "model_stream_chunk":
            delta = event.get("delta", {})
            if "Text" in delta or "text" in delta:
                text = delta.get("Text") or delta.get("text")
                print(text.get("delta", ""), end="", flush=True)
        elif event_type == "tool_call_started":
            print(f"\n[Tool] {event.get('tool', '?')} started")
        elif event_type == "async_tool_progress":
            status = event.get("status", {})
            pending = status.get("Pending") or status.get("pending") or {}
            print(f"[Tool] Progress: {pending.get('progress', 0):.0%}")
        elif event_type == "async_tool_completed":
            print(f"[Tool] Completed: {event.get('output', {})}")
        elif event_type == "tool_call_completed":
            print(f"[Tool] Result: {event.get('output', {})}")
        elif event_type == "run_completed":
            print(f"\n[Done] {event.get('output', '')}")
        elif event_type == "run_failed":
            print(f"\n[Error] {event.get('error', '')}")
    print()
