import asyncio
import inspect

import orchest
import pytest


class FakeNativeStream:
    def __init__(self, finalize_error: BaseException | None = None) -> None:
        self.finished = False
        self.finalized = 0
        self.finalize_error = finalize_error
        self.chunks: list[bytes] = []

    async def finalize(self) -> None:
        if self.finalize_error is not None:
            raise self.finalize_error
        self.finalized += 1

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


def test_completion_preserves_structured_model_error(monkeypatch) -> None:
    env_name = "ORCHEST_TEST_MISSING_COMPLETION_KEY"
    monkeypatch.delenv(env_name, raising=False)

    with pytest.raises(orchest.ModelError) as caught:
        orchest.complete(
            model="deepseek/deepseek-flash",
            user="hello",
            api_key_env=env_name,
        )

    assert caught.value.code == "missing_api_key"
    assert caught.value.status is None
    assert caught.value.provider is None
    assert caught.value.retry_after_secs is None


def test_transcribe_preserves_structured_protocol_error() -> None:
    with pytest.raises(orchest.ModelError) as caught:
        orchest.transcribe(
            b"audio",
            format="wav",
            provider="missing",
            model="model",
            api_key="test-key",
        )

    assert caught.value.code == "no_matching_provider"
    assert caught.value.diagnostic_metadata is None


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


def test_transcribe_rejects_combined_provider_model() -> None:
    with pytest.raises(RuntimeError, match="pass the model separately"):
        orchest.transcribe(
            b"audio",
            format="wav",
            provider="aliyun/qwen-audio-3.0-asr-flash",
            api_key="test-key",
        )


def test_transcribe_rejects_model_without_provider() -> None:
    with pytest.raises(RuntimeError, match="model requires provider"):
        orchest.transcribe(b"audio", format="wav", model="nova-3", api_key="k")


def test_start_asr_stream_rejects_combined_provider_model() -> None:
    async def run() -> None:
        await orchest.start_asr_stream(
            format="pcm",
            sample_rate=16_000,
            on_event=lambda event: None,
            provider="deepgram/nova-3",
            api_key="test-key",
        )

    with pytest.raises(RuntimeError, match="pass the model separately"):
        asyncio.run(run())


def test_start_asr_stream_passes_provider_and_model_separately(monkeypatch) -> None:
    seen: list[tuple] = []

    async def fake_start(*args):
        seen.append(args)
        return FakeNativeStream()

    monkeypatch.setattr(orchest, "_start_asr_stream", fake_start)

    async def run() -> None:
        await orchest.start_asr_stream(
            format="pcm",
            sample_rate=16_000,
            on_event=lambda event: None,
            provider="deepgram",
            model="nova-3",
        )

    asyncio.run(run())
    (args,) = seen
    assert args[4:6] == ("deepgram", "nova-3")


def test_finalize_keeps_the_session_open(monkeypatch) -> None:
    native = FakeNativeStream()

    async def fake_start(*args):
        return native

    monkeypatch.setattr(orchest, "_start_asr_stream", fake_start)

    async def run() -> None:
        stream = await orchest.start_asr_stream(
            format="pcm", sample_rate=16_000, on_event=lambda event: None
        )
        await stream.send_audio(b"a")
        await stream.finalize()
        await stream.send_audio(b"b")

    asyncio.run(run())
    assert native.finalized == 1
    assert native.chunks == [b"a", b"b"]
    assert not native.finished


def test_finalize_unsupported_raises_model_error(monkeypatch) -> None:
    native = FakeNativeStream(
        finalize_error=orchest.ModelError("no finalize", "unsupported_option")
    )

    async def fake_start(*args):
        return native

    monkeypatch.setattr(orchest, "_start_asr_stream", fake_start)

    async def run() -> None:
        stream = await orchest.start_asr_stream(
            format="pcm", sample_rate=16_000, on_event=lambda event: None
        )
        await stream.finalize()

    with pytest.raises(orchest.ModelError) as caught:
        asyncio.run(run())
    assert caught.value.code == "unsupported_option"


def test_events_are_delivered_with_kind(monkeypatch) -> None:
    events: list[orchest.AsrStreamEvent] = [
        {"kind": "draft", "text": "he", "segment_id": "s0"},
        {"kind": "stable", "text": "hello", "segment_id": "s0", "update": "snapshot"},
        {"kind": "turn", "segment_id": "s0"},
        {"kind": "session_closed"},
    ]
    received: list[orchest.AsrStreamEvent] = []

    async def fake_start(*args):
        for event in events:
            args[2](event)
        return FakeNativeStream()

    monkeypatch.setattr(orchest, "_start_asr_stream", fake_start)

    async def run() -> None:
        stream = await orchest.start_asr_stream(
            format="pcm", sample_rate=16_000, on_event=received.append
        )
        await stream.wait()

    asyncio.run(run())
    assert [event["kind"] for event in received] == [
        "draft",
        "stable",
        "turn",
        "session_closed",
    ]
