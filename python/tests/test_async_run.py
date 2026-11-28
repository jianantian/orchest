import asyncio
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from orchest import Agent, AgentError


def test_async_run_streams_and_releases_its_agent() -> None:
    release_model = threading.Event()

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            self.rfile.read(int(self.headers["Content-Length"]))
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            release_model.wait(10)
            chunks = [
                ("message_start", {"message": {"usage": {"input_tokens": 1}}}),
                ("content_block_start", {"content_block": {"type": "text"}}),
                ("content_block_delta", {"delta": {"type": "text_delta", "text": "done"}}),
                ("content_block_stop", {}),
                ("message_delta", {"delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 1}}),
                ("message_stop", {}),
            ]
            try:
                for event, data in chunks:
                    self.wfile.write(f"event: {event}\ndata: {json.dumps(data)}\n\n".encode())
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    def agent() -> Agent:
        return Agent(
            name="async-run-test",
            model="anthropic/claude-sonnet-4-6",
            system_prompt="Answer briefly.",
            api_url=f"http://127.0.0.1:{server.server_port}",
            api_key="local-test",
        )

    async def scenario() -> None:
        first = agent()
        with pytest.raises(RuntimeError, match="invalid .*messages.* entry"):
            await first.start_run("invalid", messages=[{}])
        async with await first.start_run("one") as run:
            with pytest.raises(AgentError) as pending:
                await run.wait()
            assert pending.value.code == "run_events_pending"
            assert (await anext(run))["type"] == "run_started"
            pending_read = asyncio.create_task(anext(run))
            await asyncio.sleep(0)
            pending_read.cancel()
            with pytest.raises(asyncio.CancelledError):
                await pending_read

            with pytest.raises(AgentError) as conflict:
                await first.start_run("two")
            assert conflict.value.code == "run_active"
            with pytest.raises(AgentError) as conflict:
                first.run_sync("two")
            assert conflict.value.code == "run_active"
            with pytest.raises(AgentError) as conflict:
                first.run_stream("two", lambda _: None)
            assert conflict.value.code == "run_active"

            other = await agent().start_run("independent")
            await other.aclose()
            await other.aclose()
            assert first.abort("timeout") is True
            assert first.abort("timeout") is False
            events = [event async for event in run]
            assert events[-1]["type"] == "run_aborted"
            assert events[-1]["cancel_reason"] == "timeout"
            assert await run.wait() == events[-1]

        release_model.set()
        async with await first.start_run("again") as run:
            async for event in run:
                if event["type"] == "run_completed":
                    with pytest.raises(AgentError) as pending:
                        await run.wait()
                    assert pending.value.code == "run_events_pending"
            assert (await run.wait())["type"] == "run_completed"

        def fail_callback(_event: object) -> None:
            raise LookupError("callback failure")

        with pytest.raises(LookupError, match="callback failure"):
            await asyncio.to_thread(first.run_stream, "legacy stream", fail_callback)
        legacy = await asyncio.to_thread(first.run_sync, "legacy")
        assert legacy[-1]["type"] == "run_completed"

    try:
        asyncio.run(asyncio.wait_for(scenario(), timeout=10))
    finally:
        release_model.set()
        server.shutdown()
        thread.join()
        server.server_close()
