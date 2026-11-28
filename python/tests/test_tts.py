"""TTS conformance through the installed wheel and shared WSS fixture."""

import asyncio
import json
import os
from pathlib import Path
import sys
import time

import pytest

from orchest import ModelError, start_tts_duplex, stream_synthesize, synthesize


async def run_fixture():
    local = Path(__file__).with_name("sdk-tts-server.py")
    fixture = local if local.exists() else (
        Path(__file__).parents[2] / "crates/orchest/tests/fixtures/sdk-tts-server.py"
    )
    process = await asyncio.create_subprocess_exec(
        sys.executable, str(fixture), stdout=asyncio.subprocess.PIPE
    )
    try:
        startup = json.loads(await asyncio.wait_for(process.stdout.readline(), 10))
    except BaseException:
        process.kill()
        await process.wait()
        raise
    return process, {
        "provider": "minimax", "voice": "fixture", "format": "mp3",
        "api_key": "fixture-key", "api_url": startup["endpoint"] + "/one-shot",
        "provider_options": {"ca_cert_pem": startup["ca"]},
    }


def test_tts_from_installed_native_package():
    async def scenario():
        process, options = await run_fixture()
        try:
            audio = b"\x00a\xff"
            result = await synthesize("Hello.", **options)
            assert result["audio"] == audio
            assert result["format"] == "mp3"
            assert result["usage"]["billed_characters"] == 6
            assert result["usage"]["audio_duration_ms"]["value"] == 125
            assert await process.stdout.readline() == b"closed\n"

            async with await stream_synthesize("Hello.", **options) as stream:
                events = [event async for event in stream]
                assert [event["kind"] for event in events] == ["audio", "done"]
                assert events[0]["audio"] == audio
                assert events[0]["context_id"] is None
                assert (await stream.usage())["outcome"] == "completed"
            assert await process.stdout.readline() == b"closed\n"

            options["api_url"] = options["api_url"].replace("/one-shot", "/duplex")
            async with await start_tts_duplex(**options) as stream:
                seen = asyncio.Queue()

                async def consume():
                    async for event in stream:
                        await seen.put(event)

                async def expect(kind, **fields):
                    event = await seen.get()
                    assert event["kind"] == kind
                    assert all(event[key] == value for key, value in fields.items())

                reader = asyncio.create_task(consume())
                await stream.append("First.")
                await expect("audio", audio=audio, context_id=1, flush_id=None)
                await stream.flush()
                await expect("flush_accepted", context_id=1, flush_id=1)
                await expect("flushed", context_id=1, flush_id=1)
                await stream.append("Cancel.")
                await expect("audio", audio=audio, context_id=2)
                cancelling = time.perf_counter()
                await stream.cancel()
                await expect("cancelled", context_id=2)
                cancel_ms = (time.perf_counter() - cancelling) * 1000
                await stream.append("Third.")
                await expect("audio", audio=audio, context_id=3)
                closing = time.perf_counter()
                await stream.close()
                await stream.close()
                await expect("context_ended", context_id=3)
                await expect("session_closed")
                await reader
                assert (await stream.usage())["input_characters"] == 19
            assert await process.stdout.readline() == b"closed\n"

            close_ms = (time.perf_counter() - closing) * 1000

            with pytest.raises(ModelError) as unsupported:
                await start_tts_duplex(**options, options={"require_capabilities": ["words"]})
            assert unsupported.value.code == "unsupported_option"

            with pytest.raises(ModelError) as fatal:
                async with await start_tts_duplex(**options) as stream:
                    await stream.append("error")
                    async for _ in stream:
                        pass
            assert fatal.value.code == "invalid_api_key"
            assert await process.stdout.readline() == b"closed\n"

            with pytest.raises(ValueError, match="handler"):
                async with await start_tts_duplex(**options) as stream:
                    await stream.append("Abort.")
                    async for event in stream:
                        if event["kind"] == "audio":
                            aborting = time.perf_counter()
                            raise ValueError("handler")
            assert await process.stdout.readline() == b"closed\n"
            if os.environ.get("ORCHEST_TTS_BENCHMARK"):
                print(f"Python cancel_ms={cancel_ms:.2f} close_ms={close_ms:.2f} "
                      f"abort_ms={(time.perf_counter()-aborting)*1000:.2f}")
        finally:
            process.terminate()
            await process.wait()

    asyncio.run(asyncio.wait_for(scenario(), 25))


def test_tts_fixture_performance():
    """Optional fixed load. Timing results do not gate slower CI machines."""
    if not os.environ.get("ORCHEST_TTS_BENCHMARK"):
        pytest.skip("set ORCHEST_TTS_BENCHMARK to measure the fixture")

    async def scenario():
        process, options = await run_fixture()
        try:
            for pause in (0, 100) * 3:
                start = time.perf_counter()
                first = None
                total = 0
                async with await stream_synthesize("benchmark", **options) as stream:
                    async for event in stream:
                        if event["kind"] != "audio":
                            continue
                        if first is None:
                            first = time.perf_counter() - start
                            await asyncio.sleep(pause / 1000)
                        total += len(event["audio"])
                assert total == 128 * 4096
                print(f"Python pause_ms={pause} first_ms={first*1000:.2f} "
                      f"total_ms={(time.perf_counter()-start)*1000:.2f}")
        finally:
            process.terminate()
            await process.wait()

    asyncio.run(asyncio.wait_for(scenario(), 25))
