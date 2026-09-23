# orchest-provider-core

**Internal crate. Not for direct use.**

Shared building blocks for the [Orchest](https://github.com/jianantian/orchest) provider implementation crates: HTTP client, retry, SSE/WebSocket scaffolding, auth strategies, OSS signing, telemetry and the registry types. This crate is published only because
[`orchest-provider`](https://crates.io/crates/orchest-provider) depends on it.
Its API has no SemVer guarantee, and `orchest-provider` pins it to an exact
version. Depend on `orchest-provider` instead.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
