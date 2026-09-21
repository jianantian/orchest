# v0.17 Live Decisions Evidence

- revision: `5ca9683` (base `23778fb` plus the `js/index.js` ESM export fix)
- date: 2026-09-21 (UTC 16:12)
- endpoint: `https://openrouter.ai/api/alpha/decisions` (provider default, no override)
- model requested: `openrouter/~typesafe/jev-latest`; model returned: `typesafe/jev-1.13-20260917`
- provider: `TypeSafe`
- credentials: `OPENROUTER_API_KEY` — key material and raw request payloads are not committed
- outcome: 3/3 SDKs returned structured answers; fixture tests, deterministic suites and this
  live run stay separate evidence layers

## Commands

| SDK | command | exit |
| --- | --- | --- |
| Rust | `cargo run -q -p orchest --example decisions` | 0 |
| Python | `maturin develop` then `.venv/bin/python examples/python/providers/decisions.py` | 0 |
| TypeScript | `node <examples/typescript/providers/decisions.ts copied as .mts>` | 0 |

## Observed answers (same customer-support batch)

| SDK | boolean `probability` | choice | choice `confidence` | score | score `confidence` | application branch taken |
| --- | --- | --- | --- | --- | --- | --- |
| Rust | 0.96 | `billing` | 0.83 | 1.04 | 0.94 | `Route to billing` (confidence ≥ 0.8) |
| Python | 0.92 | `billing` | 1.0 | 1.88 | 0.83 | `Escalate to a human` (urgency ≥ 0.8) |
| TypeScript | 0.92 | `billing` | 1.0 | 1.90 | 0.85 | `Escalate to a human` (urgency ≥ 0.8) |

## Usage and identity

| SDK | response id | input_tokens | output_tokens | cost_usd |
| --- | --- | --- | --- | --- |
| Rust | `gen-dec-1790007125-Z5XABi4sUXW6Rxafe56t` | 439 | 73 | 0.000018438 |
| Python | `gen-dec-1790007129-mCpmKnotypUiFnusTH1m` | 409 | 61 | 0.000017178 |
| TypeScript | `gen-dec-1790007131-B7bnQRLhQmsdRgHUryNs` | 409 | 61 | 0.000017178 |

Total live spend: ≈ 0.0000537 USD for three calls.

## Notes

- The boolean answer arrived as a float probability (0.92–0.96), not a bool; the score as a
  fractional level (1.04–1.90); Choice carried `probabilities` plus `confidence`. Optional
  fields the provider supplied (score `legend`, `usage.cost_usd`) were preserved, not synthesized.
- The Python example reported the structured score legend
  `{"0":"Low","1":"Moderate","2":{"escalate":true,"label":"High"}}`, i.e. nested description
  objects survive the round trip instead of being stringified.
- TypeScript: the repo ships no TS runner (`type: commonjs`, no tsconfig, no devDependencies), so
  the example was executed as an ESM copy inside the package. That path exposed the missing ESM
  named exports fixed in `5ca9683`; `import { decide } from "@orchest/sdk"` now resolves.
- Raw responses are not committed. The values above are the sanitized extract required to
  recompute the application-side branches.
