import asyncio
import json
import sqlite3
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from orchest import Agent, AgentError, EvidenceStore


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


def test_async_run_streams_and_releases_its_agent(tmp_path: Path) -> None:
    release_model = threading.Event()
    fixture = load_fixture()
    requests = []
    evidence_attempts = 0

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            nonlocal evidence_attempts
            requests.append(body)
            if body["messages"][-1]["content"][0].get("text") == "retry evidence":
                evidence_attempts += 1
                if evidence_attempts == 1:
                    self.send_response(500)
                    self.end_headers()
                    return
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
            delegates = asks_for_tool and body["tools"][0]["name"] == "delegate"
            try:
                for event, data in chunks:
                    if delegates and event == "content_block_start":
                        data = {"content_block": {**data["content_block"], "name": "delegate"}}
                    if delegates and event == "content_block_delta":
                        data = {"delta": {**data["delta"], "partial_json": '{"question":"fixture"}'}}
                    self.wfile.write(f"event: {event}\ndata: {json.dumps(data)}\n\n".encode())
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    def agent(*, retry=False) -> Agent:
        return Agent(
            name="async-run-test",
            model="anthropic/claude-sonnet-4-6",
            system_prompt="Answer briefly.",
            api_url=f"http://127.0.0.1:{server.server_port}",
            api_key="local-test",
            retry=retry,
        )

    async def request_for(run):
        async for event in run:
            if event["type"] == "approval_requested":
                return event["request_id"]
        raise AssertionError("approval was not requested")


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

        parent = agent()
        parent.register_agent_tool("delegate", "Call the configured child", agent())
        async with await parent.start_run("delegate a fixture request") as run:
            forwarded = [
                event["event"] async for event in run if event["type"] == "sub_agent_event"
            ]
            assert forwarded[0]["type"] == "run_started"
            assert forwarded[-1]["type"] == "run_completed"
            assert forwarded[-1]["output"] == "done"
            assert (await run.wait())["type"] == "run_completed"

        def fail_callback(_event: object) -> None:
            raise LookupError("callback failure")

        with pytest.raises(LookupError, match="callback failure"):
            await asyncio.to_thread(first.run_stream, "legacy stream", fail_callback)
        legacy = await asyncio.to_thread(first.run_sync, "legacy")
        assert legacy[-1]["type"] == "run_completed"

    async def session_scenario() -> None:
        path = tmp_path / "sessions.sqlite"
        session = {"sqlite_path": str(path), "session_id": "shared"}

        async def expect_error(code, operation):
            with pytest.raises(AgentError) as error:
                await operation()
            assert error.value.code == code

        await expect_error("session_not_configured", lambda: agent().resume_session())
        await expect_error("session_not_found", lambda: agent().resume_session(session))
        await expect_error("session_storage_error", lambda: agent().start_run(
            "bad path", session={**session, "sqlite_path": str(tmp_path / "missing" / "sessions.sqlite")}
        ))

        async def drain(run):
            async with run:
                async for _ in run:
                    pass
                assert (await run.wait())["type"] == "run_completed"

        corrupt = tmp_path / "unreadable.sqlite"
        corrupt.write_bytes(b"invalid sqlite")
        corrupt_session = {**session, "sqlite_path": str(corrupt)}
        await expect_error("session_storage_error", lambda: agent().start_run("bad store", session=corrupt_session))
        # Truncate in place to exercise lease release for the same inode.
        corrupt.write_bytes(b"")
        await drain(await agent().start_run("reinitialized", session=corrupt_session))

        release_model.clear()
        async with await agent().start_run("original", session=session) as first:
            async for event in first:
                if event["type"] == "model_call_started":
                    break
            assert first.capabilities()["resume"] is True
            alias = tmp_path / "alias.sqlite"
            alias.symlink_to(path)
            await expect_error("session_writer_conflict", lambda: agent().start_run(
                "duplicate", session={**session, "sqlite_path": str(alias)}
            ))
            await expect_error("session_writer_conflict", lambda: agent().resume_session(session))

            async with await agent().start_run("parallel", session={**session, "session_id": "other"}) as other:
                assert (await anext(other))["type"] == "run_started"

            # Hold the database's SQLite transaction in another process. A
            # transaction in the SDK's own process shares that process's OS locks.
            gate = await asyncio.create_subprocess_exec(
                sys.executable, "-u", "-c", """
import sqlite3
import subprocess, sys

with sqlite3.connect(sys.argv[1]) as conn:
    conn.execute("BEGIN EXCLUSIVE")
    print("locked", flush=True)
    sys.stdin.readline()
""",
                str(path),
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
            )
            try:
                assert await asyncio.wait_for(gate.stdout.readline(), 5) == b"locked\n"
                release_model.set()
                async for event in first:
                    if event["type"] == "model_call_completed":
                        break
                await asyncio.wait_for(expect_error(
                    "session_writer_conflict", lambda: agent().resume_session(session)
                ), timeout=2)
            finally:
                await gate.communicate(b"release\n")
            async for _ in first:
                pass
            assert (await first.wait())["type"] == "run_completed"

        with sqlite3.connect(path) as conn:
            before = conn.execute(
                "SELECT event_json FROM session_events WHERE session_id = ? ORDER BY seq",
                (session["session_id"],),
            ).fetchall()
        await expect_error("session_already_exists", lambda: agent().start_run("duplicate", session=session))
        with sqlite3.connect(path) as conn:
            assert conn.execute(
                "SELECT event_json FROM session_events WHERE session_id = ? ORDER BY seq",
                (session["session_id"],),
            ).fetchall() == before
        await drain(await agent().resume_session(session))
        assert [message["role"] for message in requests[-1]["messages"]].count("user") == 1
        await drain(await agent().resume_session(session, "follow-up"))
        user_turns = [message for message in requests[-1]["messages"] if message["role"] == "user"]
        assert sum(block.get("text") == "follow-up" for message in user_turns for block in message["content"]) == 1
        await drain(await agent().start_run("isolated", session={**session, "sqlite_path": str(tmp_path / "other.sqlite")}))

        recovered = {**session, "session_id": "process-exit"}
        child = await asyncio.create_subprocess_exec(
            sys.executable, "-u", "-c", """
import asyncio, sys
from orchest import Agent

def probe() -> str:
    '''Return a read-only fixture value.'''
    return 'ok'

async def main():
    agent = Agent(name="child", model="anthropic/claude-sonnet-4-6",
                  system_prompt="Answer briefly.", api_url=sys.argv[1], api_key="local-test")
    agent.register_tool(probe, approval="always")
    run = await agent.start_run("child input", session={"sqlite_path": sys.argv[2], "session_id": sys.argv[3]})
    async for event in run:
        if event["type"] == "approval_requested":
            print(event["request_id"], flush=True)
            await asyncio.Event().wait()

asyncio.run(main())
""",
            f"http://127.0.0.1:{server.server_port}", str(path), recovered["session_id"],
            stdout=asyncio.subprocess.PIPE,
        )
        try:
            previous_id = (await asyncio.wait_for(child.stdout.readline(), 10)).decode().strip()
            assert previous_id
            await expect_error("session_writer_conflict", lambda: agent().resume_session(recovered))
        finally:
            if child.returncode is None:
                child.kill()
            await child.wait()

        calls = []

        def probe() -> str:
            """Return a read-only fixture value."""
            calls.append("probe")
            return "ok"

        restored = agent()
        restored.register_tool(probe, approval="always")
        async with await restored.resume_session(recovered) as run:
            new_id = await request_for(run)
            assert new_id != previous_id
            await expect_error(
                "request_not_found", lambda: run.respond(previous_id, {"decision": "deny"})
            )
            assert calls == []
            await run.respond(new_id, {"decision": "approve"})
            async for _ in run:
                pass
            assert (await run.wait())["type"] == "run_completed"
            assert calls == ["probe"]

    async def evidence_scenario() -> None:
        path = str(tmp_path / "evidence.sqlite")
        session = {"sqlite_path": str(tmp_path / "logs.sqlite"), "session_id": "evidence"}
        store = await EvidenceStore.open(path)

        async def drain(run):
            async with run:
                run_id = (await anext(run))["run_id"]
                async for _ in run:
                    pass
                result = await run.wait()
                assert result["type"] == "run_completed", result
                return run_id

        async def expect_error(code, operation):
            with pytest.raises(AgentError) as error:
                await operation()
            assert error.value.code == code

        def change_fixture(operation, database, *args):
            script = Path(__file__).with_name("sdk-session-evidence.py")
            if not script.exists():
                script = Path(__file__).parents[2] / "crates/orchest/tests/fixtures/sdk-session-evidence.py"
            subprocess.run([sys.executable, str(script), operation, database, *args], check=True)

        await drain(await agent().start_run("default off", session=session))
        reader = await store.authorize(**session)
        assert (await reader.read())["records"] == []
        enabled = {**session, "evidence": store}
        await expect_error("session_storage_error", lambda: agent().start_run(
            "shared database", session={**enabled, "sqlite_path": path}
        ))
        captured_id = await drain(await agent(retry=True).resume_session(enabled, "retry evidence"))
        await drain(await agent().resume_session(session, "off again"))
        page = await reader.read(limit=1)
        cursor = page["next_cursor"]
        assert cursor is not None
        await expect_error("evidence_invalid_page_size", lambda: reader.read(limit=65))
        await expect_error("evidence_invalid_cursor", lambda: reader.read(cursor + "x"))

        other = {**session, "sqlite_path": str(tmp_path / "other-logs.sqlite")}
        await drain(await agent().start_run("different store", session=other))
        other_reader = await store.authorize(**other)
        await expect_error("evidence_invalid_cursor", lambda: other_reader.read(cursor))
        assert (await other_reader.read())["records"] == []

        change_fixture("seed", path)
        cursor = None
        rows = []
        empty_with_cursor = False
        invalid = 0
        while True:
            page = await reader.read(cursor, limit=1)
            rows.extend(page["records"])
            invalid += page["invalid_records"]
            next_cursor = page["next_cursor"]
            if next_cursor is None:
                break
            assert next_cursor != cursor
            empty_with_cursor |= not page["records"]
            cursor = next_cursor
        assert empty_with_cursor and invalid == 1
        starts = {row["attempt_id"]: row for row in rows if row["kind"] == "attempt_started"}
        finishes = {row["attempt_id"]: row for row in rows if row["kind"] == "attempt_finished"}
        assert len(starts) == 3 and len(finishes) == 2
        assert {row["outcome"] for row in finishes.values()} == {"returned", "adapter_error"}
        assert {row["run_id"] for row in finishes.values()} == {captured_id}
        assert {starts[attempt_id]["step"] for attempt_id in finishes} == {next(iter(finishes.values()))["step"]}
        assert {row["ordinal"] for attempt_id, row in starts.items() if attempt_id not in finishes} == {99}
        for row in starts.values():
            assert set(row) == {"kind", "attempt_id", "run_id", "step", "ordinal", "purpose", "provider", "model"}
        for row in finishes.values():
            assert set(row) == {"kind", "attempt_id", "run_id", "step", "outcome"}

        change_fixture("delete", session["sqlite_path"], session["session_id"])
        await expect_error("evidence_session_unavailable", reader.read)
        await drain(await agent().start_run("replacement", session=enabled))
        await expect_error("evidence_session_unavailable", reader.read)
        replacement = await store.authorize(**session)
        assert len((await replacement.read())["records"]) == 2
        await replacement.revoke()
        await expect_error("evidence_session_unavailable", replacement.read)
        await expect_error("evidence_session_unavailable", lambda: store.authorize(**session))

        replaced_session = {**session, "sqlite_path": str(tmp_path / "replaceable.sqlite")}
        setup = {**replaced_session, "evidence": store}
        await drain(await agent().start_run("before file replacement", session=setup))
        old_reader = await store.authorize(**replaced_session)
        assert len((await old_reader.read())["records"]) == 2
        Path(replaced_session["sqlite_path"]).replace(tmp_path / "previous.sqlite")
        await expect_error("evidence_session_unavailable", old_reader.read)
        await drain(await agent().start_run("after file replacement", session=setup))
        await expect_error("evidence_session_unavailable", old_reader.read)
        new_reader = await store.authorize(**replaced_session)
        assert len((await new_reader.read())["records"]) == 2

        await drain(await agent().resume_session({**other, "evidence": store}, "before evidence fault"))
        change_fixture("break", path)
        await expect_error("evidence_storage_error", other_reader.read)
        await drain(await agent().resume_session({**other, "evidence": store}, "after evidence fault"))

    async def verify() -> None:
        await scenario()
        await session_scenario()
        await evidence_scenario()

    try:
        asyncio.run(asyncio.wait_for(verify(), timeout=25))
    finally:
        release_model.set()
        server.shutdown()
        thread.join()
        server.server_close()
