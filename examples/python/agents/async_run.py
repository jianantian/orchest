"""Consume a run before waiting for its final result."""

import asyncio

from orchest import Agent, AgentError


async def main() -> None:
    agent = Agent(
        name="async-example",
        model="anthropic/claude-sonnet-4-6",
        system_prompt="Answer briefly.",
    )
    async with await agent.start_run("Why does backpressure matter?") as run:
        try:
            await run.wait()
        except AgentError as error:
            if error.code != "run_events_pending":
                raise

        async for event in run:
            print(event)
        print(await run.wait())

    # Context exit closes the receiver and cancels an unfinished run.
    async with await agent.start_run("Explain again with an example.") as run:
        async for event in run:
            print(event)
            break


if __name__ == "__main__":
    asyncio.run(main())
