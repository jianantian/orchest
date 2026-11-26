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
orchest = "0.2"
orchest-provider = { version = "0.2", features = ["llm"] }
```

See the [quickstart](https://github.com/jianantian/orchest/blob/main/docs/guide/quickstart.md) and the
[Rust examples](https://github.com/jianantian/orchest/tree/main/examples/rust).

## Local Session evidence

Enable the Rust `session-evidence` feature to inspect model attempts for a run
that uses a SessionStore. Use a dedicated evidence database in a host-controlled
directory for each SessionStore. For a config and store you already use:

```rust,ignore
use orchest::session::evidence::{EvidenceLimits, EvidenceStore};

let database = EvidenceStore::open("private/session-evidence.sqlite", EvidenceLimits::default()).await?;
let capture = database.bind(session_store.clone(), "my-session");
let config = config
    .with_session_store(session_store, "my-session")
    .with_session_evidence(capture.clone());
// Start or resume the run with config, then:
let page = capture.read(None, 64).await?;
```

Capture is off until attached and defaults to metadata when attached. Use
`EvidenceSession::with_content_policy` to authorize specific bounded fields,
then pass relevant pages to `reconstruct_input`. A missing or withheld field
produces an incomplete-input error. Call `revoke` before deleting the Session
if local evidence must be removed immediately. Revocation removes records from
this store, including their indexes. The host controls any copies already read.

Evidence capture is best effort. A lost process, a slow disk or a full database
can leave gaps. Evidence reads and revocation operate independently of
Session checkpoints. Inspect the Session event reference and its matching
worker watermark with `verify_event` to confirm a durable association.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
