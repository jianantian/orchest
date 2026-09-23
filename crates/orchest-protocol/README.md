# orchest-protocol

The shared protocol that every [Orchest](https://github.com/jianantian/orchest) crate and consumer speaks:

- the content model (`ContentBlock`);
- the capability traits: `ChatModel`, `Asr`, `Tts`, `VoiceManager`,
  `RealtimeSession`, `GenTask`;
- the unified `StreamEvent` model and `CapabilityDescriptor`;
- the unified `ProtocolError`.

Depend on it together with
[`orchest-provider`](https://crates.io/crates/orchest-provider) to select
providers, or with [`orchest`](https://crates.io/crates/orchest) to run
agents.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
