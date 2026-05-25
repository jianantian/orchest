# Orchest

**Skill-first agent runtime.** Rust core, Python/TypeScript SDKs.  
Aligns with the [Anthropic Agent Skills](https://agentskills.io) open standard.

Orchest is the engine, not the product — it handles the agent loop, state management, event streaming, tool dispatch, and skill loading. You bring the tools, the system prompt, and the model adapter.

---

## Design

- **Skill-first** — progressive disclosure via filesystem-organized `SKILL.md` packages
- **MCP-native** — MCP is a transport protocol for tools, not a separate tool type
- **Provider-agnostic** — Anthropic, OpenAI, DeepSeek, OpenRouter through a single `ModelAdapter` trait
- **Streaming by default** — every model response chunk and tool result is an event
- **Minimal core** — the runtime does loop + state + events; capabilities live in tools and skills

Tool / MCP / Skill are distinct layers:
| Layer | What | Role |
|-------|------|------|
| Tool | Capability | What the model can invoke |
| MCP | Protocol | How tools are discovered and connected |
| Skill | Knowledge | How-to instructions, references, optional bundled scripts |

---

## Quick start

### Python

```bash
uvx maturin develop
```

```python
from agent_runtime import Agent

agent = Agent(
    model="anthropic/claude-sonnet-4-6",
    system_prompt="You are a helpful assistant.",
    api_key_env="ANTHROPIC_API_KEY",
)

for event in agent.run("What is 2 + 2?"):
    if event["type"] == "model_stream_chunk":
        print(event["delta"]["Text"]["delta"], end="")
```

### TypeScript

```bash
npm install && npm run build:native
```

```typescript
import { Agent } from "@orchest/agent-runtime";

const agent = new Agent({
  model: "anthropic/claude-sonnet-4-6",
  systemPrompt: "You are a helpful assistant.",
  apiKeyEnv: "ANTHROPIC_API_KEY",
});

for (const event of agent.runSync("What is 2 + 2?")) {
  if (event.type === "model_stream_chunk") {
    process.stdout.write(event.delta.Text.delta);
  }
}
```

### Provider format

Model strings use `provider/model` syntax:

| Prefix | Provider |
|--------|----------|
| `anthropic/` | Anthropic (Claude) |
| `openai/` | OpenAI (GPT, o-series) |
| `deepseek/` | DeepSeek |
| `openrouter/` | OpenRouter (any routed model) |

API keys are read from the standard env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.) or passed explicitly.

---

## Project structure

```
crates/
  agent-runtime-core/     # Pure Rust — run loop, tool registry, skill loading, events
  agent-runtime-providers/ # Model adapters (Anthropic, OpenAI, DeepSeek, OpenRouter)
  agent-runtime-py/        # PyO3 binding
  agent-runtime-node/      # napi-rs binding
python/
  agent_runtime/           # Python package with typed stubs
js/                        # TypeScript SDK with native bindings
examples/                  # Runnable demos (Python, TypeScript, Rust)
docs/                      # Design docs, iteration PRDs, polaris principles
skills/                    # Example skill packages
```

---

## Development

```bash
# Rust
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check

# Python
uvx maturin develop
.venv/bin/python -m pytest python/tests/ -v

# Node.js
npm run build:native
```

See [`AGENTS.md`](./AGENTS.md) for conventions, [`WORKFLOW.md`](./WORKFLOW.md) for the development workflow.

---

## Documentation

| Document | Audience |
|----------|----------|
| [`docs/overview.md`](./docs/overview.md) | Everyone — concepts and philosophy |
| [`docs/polaris/`](./docs/polaris/) | Contributors — design principles, non-goals, observability contract |
| [`docs/iteration/`](./docs/iteration/) | Contributors — per-version PRDs and issue specs |
| [`docs/review/`](./docs/review/) | Contributors — code review findings |
| [`AGENTS.md`](./AGENTS.md) | AI agents and contributors — working conventions |

---

## License

UNLICENSED — internal development.
