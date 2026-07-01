# Issue 007: Visual + remaining modality migration

## Background

aigc (image/video/music) uses concrete `ImageGateway`-style structs, **not** a provider trait. This issue
abstracts `GenTask` from them, builds `orchest-provider-visual` (signed/polled gen), routes minimax music to
`orchest-provider-http`, and reconciles the three pricing models (token-tier / duration / asset) in core.
`agent-runtime-aigc-providers` is dissolved.

## Goal / Scope

In scope:

- `orchest-provider-visual`: volc-visual, aliyun, crazyrouter, renderful as `GenTask` impls (signed / OSS / poll).
- minimax music → `orchest-provider-http` (REST/Bearer).
- Abstract `GenTask` (submit/poll/fetch) from `ImageGateway` + the video gateway; asset storage via core.
- Reconcile pricing: `ModelPricing` (token-tier), ASR duration (`AsrUsage`), asset — one accounting surface in core.
- Delete `agent-runtime-aigc-providers`.

Out of scope: cleanup/bindings (Issue 008).

## Acceptance Criteria

- [ ] `GenTask` is implemented by the gen providers; `ImageGateway`/video behavior preserved (existing tests green).
- [ ] `agent-runtime-aigc-providers` is dissolved (gen → visual, music → http); ASR/TTS already moved (006).
- [ ] Pricing across token / duration / asset is reconciled in core with tests.
- [ ] The **Chameleon ruler** holds: a `ChatModel` emitting `Image` in its output stream needs no `GenTask`.

## Notes

Depends on 002/003/004. The `GenTask` shape comes from Issue 001. Pricing reconciliation is the subtle part.
