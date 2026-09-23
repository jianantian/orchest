# orchest-provider

The provider registry for [Orchest](https://github.com/jianantian/orchest). Select LLM, ASR, TTS, realtime
and image/video generation providers by **capability query** or **identity
pick**. The implementation crates and wire dialects stay behind this crate.

```rust
use orchest_provider::Registry;
use orchest_protocol::Modality;

let reg = Registry::with_builtin(); // whatever features enable
let candidates = reg.chat().accepts([Modality::Text, Modality::Image]).thinking().list();
let _ = reg.asr().provider("volcengine").bidirectional().select();
let _ = reg.chat().id("openai/gpt-5.4").select();
```

## Features

The default feature set registers no providers. Features choose which
implementations get compiled in:

| Feature | Pulls in |
| --- | --- |
| `http` | REST/SSE providers |
| `stream` | WebSocket providers |
| `visual` | signed/polled image and video generation |
| `llm`, `decision` | `http` |
| `asr`, `tts` | `http` + `stream` |
| `realtime` | `stream` |
| `gen` | `http` + `visual` |
| `testing` | deterministic in-process `Asr`/`Tts` fakes |

Custom providers can be registered through `Registry::register_*`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
