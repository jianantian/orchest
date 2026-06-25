# 004 · Guardrail policy examples

## Background

Research suggested a full policy engine, but Orchest's core design prefers extension through `Guardrail`.

## Goal

Provide examples for complex permission policies as application-layer guardrails.

## Acceptance Criteria

- [x] Example covers multi-level authority or risk classification using existing guardrails.
- [x] Docs explain why this remains app-layer.
- [x] Example compiles and runs in CI.
- [x] Example demonstrates approval integration using current `Approval` / guardrail APIs after deprecated API removal.
- [x] Any proven core gap is documented as a follow-up issue with evidence from the example.

## Notes

Added `guardrail_authority_policy`, which demonstrates application-layer
authority policy using `ToolInputGuardrail` plus current `Approval::WhenRisky`
metadata. The example rejects a high-risk support-actor request, allows a
medium-risk manager request, then routes the allowed side-effecting tool through
the approval gate.

No core gap was proven. The example supports the existing design: product policy
classification remains app-layer, while core supplies guardrail and approval
enforcement points.
