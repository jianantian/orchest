"""Consume events, answer an approval by ID, and resume a local session."""

import asyncio
import os
from pathlib import Path
from tempfile import TemporaryDirectory

from orchest import Agent, AgentError, Run, SessionOptions


def make_agent() -> Agent:
    agent = Agent(
        name="async-example",
        model="anthropic/claude-sonnet-4-6",
        system_prompt="Answer briefly.",
        api_url=os.environ.get("ORCHEST_EXAMPLE_API_URL"),
        api_key_env="ANTHROPIC_API_KEY",
    )

    def probe() -> str:
        """Return a small status message for the user."""
        return "The example is running."

    agent.register_tool(probe, approval="always")
    return agent


async def consume(run: Run) -> None:
    try:
        await run.wait()
    except AgentError as error:
        if error.code != "run_events_pending":
            raise

    async for event in run:
        if event["type"] == "approval_requested":
            decision = "approve" if event["tool_call"]["name"] == "probe" else "deny"
            await run.respond(event["request_id"], {"decision": decision})
        print(event["type"])

    result = await run.wait()
    if result["type"] != "run_completed":
        raise RuntimeError(result)
    print(result["output"])


async def main() -> None:
    with TemporaryDirectory() as directory:
        session: SessionOptions = {
            "sqlite_path": str(Path(directory) / "example.sqlite"),
            "session_id": "async-example",
        }
        async with await make_agent().start_run("Call probe and summarize its result.", session=session) as run:
            await consume(run)
        async with await make_agent().resume_session(session, "Summarize the last result.") as run:
            await consume(run)

    # Context exit closes the receiver and cancels any unfinished work.
    async with await make_agent().start_run("Explain backpressure.") as run:
        async for event in run:
            print(event["type"])
            break


if __name__ == "__main__":
    asyncio.run(main())
