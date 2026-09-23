# orchest

Skill-first agent runtime core for Rust. `orchest` runs the agent loop and
handles state management, event streaming, tool dispatch and skill loading.
You bring the tools, the system prompt and the model.

- **Tools** are the atomic capabilities the model can invoke.
- **MCP** is a transport for tool providers, not a separate tool type.
- **Skills** are filesystem packages (`SKILL.md` plus optional scripts),
  exposed to the agent through progressive disclosure, following the
  [Agent Skills](https://agentskills.io) open standard.

Models and other providers come from
[`orchest-provider`](https://crates.io/crates/orchest-provider); the shared
types live in [`orchest-protocol`](https://crates.io/crates/orchest-protocol).

```toml
[dependencies]
orchest = "0.1"
orchest-provider = { version = "0.1", features = ["llm"] }
```

See the [quickstart](https://github.com/jianantian/orchest/blob/main/docs/guide/quickstart.md) and the
[Rust examples](https://github.com/jianantian/orchest/tree/main/examples/rust).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
