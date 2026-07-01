# Issue 004: Registry selection API surface (PRD Decision 4)

> Records the selection surface settled in Issue 004 against real call sites, as
> PRD Decision 4 deferred. Implemented in `crates/orchest-providers`.

## The surface

A **per-capability fluent builder** off one `Registry`, where each capability
method returns a typed `Query<H>` whose terminals return concrete handles:

```rust
let reg = Registry::with_builtin();                 // features decide what's registered

// capability query → pick one
let model = reg.chat().accepts([Text, Image, Video]).thinking().select()?;   // &Entry<Box<dyn ChatModel>>

// capability + identity mixed (the spec's "bidirectional ASR from Volcengine")
let asr  = reg.asr().provider("volcengine").bidirectional().select()?;

// identity pick
let model = reg.chat().id("openai/gpt-5.4").select()?;

// list-then-choose
let candidates = reg.chat().streaming().tools().list();                       // Vec<&Entry<…>>

// select + instantiate in one step
let handle = reg.chat().id("openai/gpt-5.4").build(&config)?;                 // Box<dyn ChatModel>

// vendor facade (vendor view over dialect crates)
let asr = orchest_providers::providers::volcengine::asr(&reg, config)?;
```

## Why this shape (the constraints from Decision 4)

| Constraint (PRD Decision 4) | How the surface meets it |
|---|---|
| One mechanism serves capability-query **and** identity-pick | Both are filters on the same `Query`: `accepts/thinking/streaming/bidirectional/…` (capability) and `provider/id` (identity). |
| The two **mix** | Filters compose: `reg.asr().provider("volcengine").bidirectional()`. |
| **pick-one** and **list-then-choose** | `Query::select()` (pick-one, deterministic) and `Query::list()` (all matches, sorted). |
| Never exposes an impl crate | `Query` returns protocol trait objects (`Box<dyn ChatModel>`, …); the impl crate/dialect is never named. |
| Static filter **before** instantiation | Filtering is over `CapabilityDescriptor` (static); the factory runs only in `select()?.instantiate(cfg)` / `build(cfg)`. |

Rust has no arity overloading, so the identity shorthand is `chat().id("…")`
rather than `chat("…")` — a fluent method, not an overloaded call. This is the
one deviation from the PRD's illustrative pseudocode, and it keeps the surface
uniform (every path is `reg.<cap>()….terminal()`).

## Reconciliation of the two legacy shapes

- **LLM `ProviderRegistry`** (factory-by-provider-name, hardcoded `register(...)`
  list, returns `Box<dyn ModelAdapter>`): folded into `Query::id`/`provider`
  identity filtering + the per-capability factory in `Entry`. The hardcoded list
  is replaced by feature-gated `Registry::with_builtin()` collecting entries from
  the enabled impl crates.
- **ASR `AsrRouter.select_for_*`** (filter candidates by language, sort by
  priority, pick first): generalized into `Query::list`/`select` — filter on the
  descriptor, sort deterministically, pick first. The route-priority specialization
  becomes the deterministic `(provider, model)` ordering; richer ranking can layer
  on later without changing the surface.

## Deferred to the impl issues

- Concrete `Entry` registrations + per-vendor facade modules (005/006/007).
- Per-vendor `features` rows (`volcengine`, `minimax`, …) — Issue 004 ships the
  weight-tier (`http`/`stream`/`visual`) and capability (`llm`/`asr`/…) features only.
