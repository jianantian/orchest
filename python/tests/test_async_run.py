import asyncio
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from orchest import Agent, AgentError


def load_fixture() -> dict:
    fixture = Path(__file__).with_name("sdk-runtime-events.json")
    if not fixture.exists():
        fixture = Path(__file__).parents[2] / "crates/orchest/tests/fixtures/sdk-runtime-events.json"
    return json.loads(fixture.read_text())


def normalize(event: dict) -> dict:
    return {
        key: f"<{key}>" if key in {"run_id", "message_id"} and isinstance(value, str) else value
        for key, value in event.items()
    }


def test_async_run_streams_and_releases_its_agent() -> None:
    release_model = threading.Event()
    fixture = load_fixture()

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            release_model.wait(10)
            asks_for_tool = body.get("tools") and not any(
                block.get("type") == "tool_result"
                for message in body["messages"]
                for block in message["content"]
            )
            chunks = fixture["tool" if asks_for_tool else "complete"]
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
            events = []
            async for event in run:
                events.append(normalize(event))
                if event["type"] == "run_completed":
                    with pytest.raises(AgentError) as pending:
                        await run.wait()
                    assert pending.value.code == "run_events_pending"
            assert (await run.wait())["type"] == "run_completed"
            assert events == fixture["expected"]

        calls = []
        agents = [agent(), agent()]
        def probe() -> str:
            """Return the fixture tool result."""
            calls.append("probe")
            return "ok"

        for item in agents:
            item.register_tool(probe, approval="always")

        async def request_for(run):
            async for event in run:
                if event["type"] == "approval_requested":
                    return event["request_id"]
            raise AssertionError("approval was not requested")

        async with await agents[0].start_run("left") as left, await agents[1].start_run("right") as right:
            left_id, right_id = await asyncio.gather(request_for(left), request_for(right))
            assert left.capabilities() == agents[0].capabilities()
            for request_id, payload, code in [
                (right_id, {"decision": "approve"}, "request_not_found"),
                (left_id, {"decision": "bad"}, "invalid_response"),
            ]:
                with pytest.raises(AgentError) as error:
                    await left.respond(request_id, payload)
                assert error.value.code == code
            await left.respond(left_id, {"decision": {"type": "answer", "payload": {"ok": True}}})
            with pytest.raises(AgentError) as expired:
                await left.respond(left_id, {"decision": "approve"})
            assert expired.value.code == "request_not_found"
            await right.respond(right_id, {"decision": "deny"})
            for active in (left, right):
                assert [event async for event in active][-1]["type"] == "run_completed"
            assert calls == ["probe"]

        async with await agents[0].start_run("cancel approval") as waiting:
            await request_for(waiting)
            assert waiting.cancel("timeout")
            assert not waiting.cancel("timeout")
            assert [event async for event in waiting][-1]["cancel_reason"] == "timeout"

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
