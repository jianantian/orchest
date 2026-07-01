"""Demonstrate that Agent.run_sync releases the GIL during the Rust run loop.

Set ORCHEST_MODEL and ORCHEST_API_KEY_ENV if you do not want the defaults:

    ORCHEST_MODEL=anthropic/claude-sonnet-4-6 \
    ORCHEST_API_KEY_ENV=ANTHROPIC_API_KEY \
    python examples/python/gil_behavior.py
"""

from __future__ import annotations

import os
import threading
import time

from orchest import Agent


def main() -> None:
    ticks = 0
    stop = threading.Event()

    def ticker() -> None:
        nonlocal ticks
        while not stop.is_set():
            ticks += 1
            time.sleep(0.01)

    thread = threading.Thread(target=ticker)
    thread.start()
    try:
        agent = Agent(
            model=os.environ.get("ORCHEST_MODEL", "anthropic/claude-sonnet-4-6"),
            system_prompt="Answer in one short sentence.",
            api_key_env=os.environ.get("ORCHEST_API_KEY_ENV", "ANTHROPIC_API_KEY"),
        )
        events = agent.run_sync("Say hello.")
    finally:
        stop.set()
        thread.join()

    print(f"events={len(events)} background_ticks={ticks}")
    assert ticks > 0, "background Python thread should run while run_sync waits"


if __name__ == "__main__":
    main()
