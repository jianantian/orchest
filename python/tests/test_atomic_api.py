import asyncio
import inspect

import orchest


class FakeNativeStream:
    def __init__(self) -> None:
        self.finished = False
        self.chunks: list[bytes] = []

    async def send_audio(self, audio: bytes) -> None:
        if self.finished:
            raise RuntimeError("ASR session is finishing or closed")
        self.chunks.append(audio)

    def finish(self) -> None:
        self.finished = True

    async def wait(self) -> None:
        return None


def test_atomic_exports_are_public() -> None:
    assert callable(orchest.complete)
    assert callable(orchest.transcribe)
    assert inspect.iscoroutinefunction(orchest.start_asr_stream)
    assert orchest.AsrStream


def test_realtime_callback_error_is_rethrown_by_wait(monkeypatch) -> None:
    marker = RuntimeError("callback failed")
    native = FakeNativeStream()
    calls = 0

    async def fake_start(*args):
        callback = args[2]
        callback({"type": "first"})
        callback({"type": "suppressed"})
        return native

    def on_event(event) -> None:
        nonlocal calls
        calls += 1
        raise marker

    monkeypatch.setattr(orchest, "_start_asr_stream", fake_start)

    async def run() -> None:
        stream = await orchest.start_asr_stream(
            format="pcm",
            sample_rate=16_000,
            on_event=on_event,
        )
        assert native.finished
        try:
            await stream.wait()
        except RuntimeError as error:
            assert error is marker
        else:
            raise AssertionError("wait should rethrow the callback exception")

    asyncio.run(run())
    assert calls == 1


def test_realtime_rejects_coroutine_callback(monkeypatch) -> None:
    native = FakeNativeStream()

    async def fake_start(*args):
        args[2]({"type": "partial"})
        return native

    async def on_event(event) -> None:
        del event

    monkeypatch.setattr(orchest, "_start_asr_stream", fake_start)

    async def run() -> None:
        stream = await orchest.start_asr_stream(
            format="pcm",
            sample_rate=16_000,
            on_event=on_event,
        )
        try:
            await stream.wait()
        except TypeError as error:
            assert "synchronous" in str(error)
        else:
            raise AssertionError("wait should reject a coroutine callback")

    asyncio.run(run())
