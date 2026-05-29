# ASR Vendor Landscape — Mobile Input Method & Emerging Markets

**Status**: draft — synthesizes public pricing, payment methods, and mobile input method suitability across 14 ASR vendors.
**Target audience**: product + engineering teams evaluating ASR providers for a mobile keyboard/input method targeting emerging markets (Southeast Asia, Africa, Latin America, South Asia).

**Key framing**: This document is organized around **suitability for mobile input methods in emerging markets**, not pure ASR vendor rankings. Price, payment accessibility, and real-world procurement feasibility carry equal weight to WER.

---

## 1. Master Table — Pricing + Payment + Suitability

Prices are from public pricing pages as of 2026-05. Enterprise contracts, regional pricing, and volume discounts can shift these significantly.

| Vendor | Realtime ASR Price (approx.) | Payment Methods | Procurement Fit | Mobile IME Suitability |
|--------|------------------------------|-----------------|-----------------|------------------------|
| **Soniox** | ~$0.12/hr real-time | Token-based metering; payment details sparse — requires registration or sales contact | Hard: opaque self-serve | High: extreme low price, multi-lang/code-switch claims strong |
| **Speechmatics** | Standard $0.24/hr; Enhanced $0.56/hr | Free tier (no CC); Pro/Enterprise via card or contract | Good: free tier for PoC, enterprise for scale | High: accent coverage, multi-lang, enterprise deployment maturity |
| **Rev.ai / Rev Reverb** | Turbo $0.10/hr; Reverb $0.20/hr; Foreign Lang $0.30/hr | 5hr free trial no CC; PAYG via CC credits; invoice → wire/ACH | Good: low-friction start, then scale | Medium: low cost, but real-time IME UX needs rigorous testing |
| **ElevenLabs Scribe v2 Realtime** | $0.39/hr | CC, Apple Pay, Google Pay, India UPI; API via account credits | Excellent: widest payment surface | High: strong productization, but ASR ecosystem relatively new — test target-market accents |
| **AssemblyAI Universal-3 Pro Streaming** | $0.45/hr; diarization/prompting extra | Free credits; add CC for more; enterprise volume discounts | Good: dev-friendly, clear docs | High: keyterms, prompting, diarization → direct IME value |
| **Deepgram Flux Multilingual** | ~$0.46–0.55/hr (varies by page) | PAYG, $200 free credit, no CC required; Growth tier $4K+/yr prepaid | Excellent: no-CC start, then enterprise | High: voice-agent-ready, turn-taking, low latency → strong real-time fit |
| **Gladia Realtime** | Starter $0.75/hr; Scaling ~$0.55/hr | Stripe (Visa/MC); enterprise → bank transfer or invoice | Good: standard Stripe, enterprise path | Medium: feature-complete but price premium for IME; good multilingual all-in-one benchmark |
| **Google Cloud STT** | ~$0.96/hr (model/region dependent) | GCP billing: CC, invoice, enterprise contract | Enterprise-only in practice | Low (cost): quality/stability benchmark, unsustainable as primary IME link at public price |
| **Azure Speech** | ~$1.20/hr real-time | CC, debit, wire transfer; no virtual/prepaid cards; MS rep → default wire | Enterprise: compliance-strong, procurement-mature | Low (cost): compliance-friendly, but C-side IME cost prohibitive |
| **AWS Transcribe** | ~$1.44/hr ($0.024/min), tiered discounts | AWS default payment: CC; some regions ACH/bank | Enterprise: procurement-mature | Low (cost): enterprise fallback, public price too high for IME |
| **Alibaba Cloud (阿里云)** | ~¥1/hr (resource packs); PAYG separate | Balance, Alipay, corporate/personal online banking, UnionPay, corporate transfer | Excellent for China-based teams | High for Chinese markets; global emerging-market language quality requires separate validation |
| **Volcengine (火山引擎)** | ~¥4.5/hr large-model streaming | Alipay, WeChat Pay, personal/corporate online banking, corporate transfer | Excellent for China-based teams | Medium: Chinese/local strong; global multi-lang and overseas POPs need verification |
| **Tencent Cloud ASR (腾讯云)** | Multiple tiers (realtime/LLM/cross-border); per-product pricing | Online top-up: WeChat, Tenpay, QQ Wallet, intl CC, offline wire; Tencent Cloud International supports CC | Excellent for China + international | Medium: Chinese procurement strong; cross-border edition worth testing, language coverage to validate |
| **Baidu AI Cloud (百度智能云)** | CN/EN resource packs + promotional pricing; long-term rate per console | Balance, DuXiaoMan, Alipay, WeChat, offline wire; no corporate online banking direct | Excellent for China-based teams | Medium: Chinese-focused; lower priority for global IME primary link |

---

## 2. Payment Convenience Tiers

### Tier A — Fastest PoC (Overseas Credit Card)

These let your engineering team start testing immediately without procurement overhead:

| Vendor | Start friction | Scale friction |
|--------|---------------|----------------|
| ElevenLabs | CC, Apple Pay, Google Pay, India UPI | Account credits → enterprise |
| AssemblyAI | Free credits, add CC | Volume discounts |
| Deepgram | $200 free credit, no CC required | Growth tier $4K+/yr prepaid |
| Gladia | Stripe (Visa/MC) | Enterprise invoice/bank transfer |
| Rev.ai | 5hr free trial, no CC; PAYG CC credits | Invoice → wire/ACH |

**Caveat for China-based teams**: Overseas CC limits, corporate expense reimbursement, fapiao (发票), tax withholding, and contract entity issues can make Tier A painful at production scale despite easy PoC start.

### Tier B — Enterprise Contract / Invoice Friendly

These are the standard production path for any vendor at significant volume:

- **Google Cloud / Azure / AWS** — full enterprise procurement, SLA, compliance
- **Deepgram / Speechmatics / Gladia / Rev / AssemblyAI** Enterprise plans

Azure notably supports both CC/debit and wire transfer, but [Microsoft docs warn](https://learn.microsoft.com/en-us/azure/cost-management-billing/manage/change-credit-card) that switching payment methods post-setup can lock you into wire-only. Plan upfront.

**Advantages**: compliance, SLA, mature procurement, volume discount negotiation.
**Disadvantages**: self-serve public pricing is rarely competitive; contract cycles are long.

### Tier C — China-Team Payment Native

These are the most procurement-friendly for teams operating from China:

| Vendor | Payment Surface |
|--------|----------------|
| Alibaba Cloud | Alipay, online banking, UnionPay, corporate transfer |
| Volcengine | Alipay, WeChat Pay, personal/corporate online banking, corporate transfer |
| Tencent Cloud | WeChat, Tenpay, QQ Wallet, international CC, offline wire |
| Baidu AI Cloud | DuXiaoMan, Alipay, WeChat, offline wire |

**Advantage**: RMB settlement, fapiao, internal approval paths, contract entities — everything a China-based finance team expects.
**Disadvantage**: ASR quality for non-Chinese emerging-market languages is the variable to test; do not extrapolate Chinese-language performance to Nigerian Pidgin, Bahasa Indonesia, Brazilian Portuguese, or Russian.

### The Real-World Procurement Split

For a China-based team building a global mobile IME, reality is likely:

```
Overseas ASR vendors → best effect/price, painful procurement
Domestic cloud vendors → easy procurement, risky global quality
```

**Implication**: Technical evaluation and procurement feasibility must be scored independently. Do not collapse them into one axis.

---

## 3. Mobile Input Method Suitability Tiers

Evaluation is framed around IME-specific concerns: real-time partial streaming, finalization latency, end-of-speech detection stability, code-switching accuracy, noise robustness, weak-network recovery.

### Tier 1 — First Round Testing (Must Test)

These five cover the full IME requirement surface:

| Vendor | Primary IME Strength | Risk to Test |
|--------|---------------------|--------------|
| **Soniox** | Extreme low price; multi-lang / code-switching claims | Self-serve maturity, payment opacity |
| **Speechmatics** | Accent coverage, enterprise deployment, reasonable price | Real-time partial streaming UX |
| **Deepgram Flux** | Real-time turn-taking, low latency, voice-agent DNA | Multilingual depth beyond their marketed languages |
| **ElevenLabs Scribe v2 Realtime** | New-gen product experience, broad payment surface | ASR ecosystem maturity vs established players |
| **AssemblyAI Universal-3 Pro Streaming** | Engineering clarity: keyterms, prompting, diarization → direct IME value | Price at scale without volume discount |

Coverage matrix:

| Dimension | Strongest Candidates |
|-----------|---------------------|
| **Lowest cost** | Soniox, Speechmatics Standard, Rev |
| **Real-time UX** | Deepgram, ElevenLabs |
| **Engineering features** | AssemblyAI |

### Tier 2 — Conditional Testing

- **Gladia**: Worth testing if you want an all-in-one multilingual benchmark, but price is high for IME scale.
- **Rev.ai**: Lowest price point, but real-time IME partial/final behavior needs rigorous testing — low latency for transcription is not the same as good IME UX.

### Tier 3 — Quality Benchmarks / Enterprise Fallbacks

- **Google Cloud STT** — quality and stability gold standard. Use as evaluation benchmark.
- **Azure Speech** — compliance-strong. Enterprise fallback for regulated markets.
- **AWS Transcribe** — enterprise fallback. Public price too high for IME primary link.

### Tier 4 — China / Chinese-Language Specialized

- **Alibaba Cloud** — strongest Chinese-language ASR; global emerging-market language quality TBD.
- **Volcengine** — large-model streaming ASR; verify overseas POP latency and non-Chinese language quality.
- **Tencent Cloud** — cross-border edition worth testing for international deployment.
- **Baidu AI Cloud** — Chinese-focused; lower priority for global IME primary link.

**Rule**: These solve "boss demo in Chinese" and China-region business. Do not extrapolate to Nigeria, Indonesia, Brazil, or Russia without testing.

---

## 4. Recommended Strategy — ASR Gateway + Multi-Vendor Evaluation

Do not lock into one vendor. Build an **ASR Gateway** abstraction and run a structured multi-vendor evaluation.

### Routing Strategy (Illustrative)

```
Chinese + domestic network:
  → Alibaba / Volcengine / Tencent / Baidu / Google CN

English + emerging-market accents:
  → Soniox / Speechmatics / Deepgram / AssemblyAI / ElevenLabs

Code-switching (mixed language in single utterance):
  → Soniox / Deepgram / Gladia / Speechmatics

High-realtime AI agent / voice-first entry:
  → Deepgram Flux / ElevenLabs / AssemblyAI

Cost floor (simplest utterances, fallback):
  → Soniox / Speechmatics Standard / Rev

Quality floor (high-stakes or regulated):
  → Google / Azure / AWS
```

### Evaluation Metrics — Beyond WER

For mobile IME, WER is a hygiene metric. The real differentiators:

| Metric | Why It Matters |
|--------|---------------|
| **P95 final latency** | Time from end-of-speech to on-screen text. Users perceive anything >300ms as lag. |
| **Partial rollback rate** | How often interim text is revised before finalization. Frequent rollbacks degrade trust and typing flow. |
| **End-of-speech cutoff rate** | False finalization during natural pauses. Critical for dictation-style input. |
| **Code-switching accuracy** | Mixed-language utterances (e.g. "send the 报告 by evening"). Non-negotiable for emerging markets. |
| **Hot-word recall** | Contact names, brand names, place names. High-frequency IME need. |
| **Noise robustness** | Low-end phones, street noise, fans, motorcycles. Real-world emerging-market conditions. |
| **Weak-network recovery** | 2G/3G/unstable 4G. Core requirement for Africa, Southeast Asia, Latin America. |
| **$/1K accepted words** | More representative than $/audio-hour for IME. Accounts for silence, retries, and rejection. |
| **Payment/procurement score** | Can you actually ship at scale, not just demo? Separate axis from technical quality. |

### Recommended Evaluation Pipeline

1. **Collect target-market audio corpus**: real speakers, real environments, representative code-switching.
2. **Blind test Tier 1 vendors** on this corpus with the metrics above.
3. **Run Tier 4 vendors** as the Chinese-language baseline and "boss demo" path.
4. **Use Tier 3 vendors** (Google/Azure) as quality benchmarks — their WER sets the ceiling.
5. **Score technical quality and procurement feasibility independently**, then intersect.

---

## 5. Conclusions

### Shortlist

**PoC Round 1 — Primary**:
Soniox, Speechmatics, Deepgram Flux, ElevenLabs Scribe v2 Realtime, AssemblyAI Universal-3 Pro Streaming

**Low-cost benchmark**:
Rev.ai / Rev Reverb

**Quality benchmarks**:
Google Cloud STT, Azure Speech

**China procurement / Chinese-language**:
Alibaba Cloud, Volcengine, Tencent Cloud, Baidu AI Cloud

### Likely Production Shape

Not "one vendor to rule them all," but a routing fabric:

```
China / Chinese  → dedicated route (Alibaba/Volcengine/etc.)
Global mainstream → dedicated route (Deepgram/ElevenLabs/etc.)
Cost floor       → Soniox / Speechmatics Standard / Rev
High-experience   → Deepgram Flux / ElevenLabs
Quality fallback  → Google / Azure
```

### Immediate Next Step

Run Soniox, Speechmatics, Deepgram, ElevenLabs, and AssemblyAI against **real target-market audio with code-switching** in a blind evaluation. Run Alibaba/Volcengine for Chinese demos in parallel. Make routing decisions from data, not spec sheets.
