# Issue 008 — dependency-weight evidence

Acceptance criterion (PRD §Acceptance Criteria): `features = ["llm"]` yields a
dependency tree with **no `tokio-tungstenite` and no OSS-signing crypto**. The
weight tiers are real: enabling only the REST capability must not drag in the
websocket (`stream`) tier or its native-TLS websocket + HMAC/SHA-2 request-signing
deps.

## Command

```
cargo tree -e features -p orchest-providers --features llm
```

## Result

`llm = ["http"]`, so the wall pulls exactly the REST tier:

```
orchest-providers (features = ["llm"])
├── orchest-protocol            # the spine
└── orchest-provider-core       # shared http/sse/telemetry/auth stack
    └── (reqwest + native-tls)  # HTTPS TLS for REST — NOT websocket
    └── orchest-provider-http   # REST/SSE LLM + ASR + music dialects
```

Checks over the 743-line tree:

| Probe | Count under `--features llm` | Expected |
|---|---|---|
| `tokio-tungstenite` | 0 | 0 |
| OSS-signing crypto (`hmac`, `sha2`) | 0 | 0 |
| websocket impl crate `orchest-provider-stream` | 0 | 0 |
| gen impl crate `orchest-provider-visual` | 0 | 0 |
| REST impl crate `orchest-provider-http` | present | present |

`tokio-tungstenite`, `futures-util`, `hmac`, `sha2`, `hex`, and `base64` are
declared **optional** in `orchest-provider-core` and gated behind its `ws` / `oss`
features (`ws = ["dep:tokio-tungstenite", "dep:futures-util"]`). The `http` tier
never enables them, so `--features llm` compiles the REST path with zero websocket
or request-signing weight. `native-tls` / `tokio-native-tls` do appear — that is
reqwest's HTTPS transport for the REST calls themselves, not the websocket stack.

The `stream`-bearing capabilities (`asr = ["http","stream"]`,
`tts = ["http","stream"]`, `realtime = ["stream"]`) are where `tokio-tungstenite`
and the OSS-signing crypto enter, exactly as intended.
