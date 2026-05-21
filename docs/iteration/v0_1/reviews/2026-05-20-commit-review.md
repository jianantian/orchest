# iteration v0_1 full commit review (since 2474af940f946b0d5b5b2def42022959a15d7dc3)

## Scope clarification
This review supersedes the previous narrow review.

Reviewed **all commits merged for iteration v0_1** starting from:
- base: `2474af940f946b0d5b5b2def42022959a15d7dc3`
- range inspected: `2474af940f946b0d5b5b2def42022959a15d7dc3..815a33f`

This includes issue branches and merge commits for:
- project setup
- core types
- tool registry
- model adapter
- run loop
- budget guard
- approval gate
- async job
- skill scanner
- skill bundled tool
- builtin read_file
- python sdk
- typescript sdk
- e2e validation

## Review dimensions (iteration-level)

This review evaluates v0.1 from **two groups of dimensions**:

### A. Spec/contract dimensions
1. **Contract consistency**: type names, event naming, field semantics kept consistent across Rust/Python/TS docs.
2. **Streaming protocol completeness**: start/delta/end boundaries and usage end markers are明确 and testable.
3. **Control-path coverage**: success + failure + denied branches are all covered in acceptance and E2E.
4. **Security boundary clarity**: known v0.1 limitations are explicitly documented and observable.

### B. Code-quality dimensions (missing in previous revision, now added)
5. **Modularity & separation of concerns**: core logic stays in `agent-runtime-core`; bindings remain glue-only.
6. **Error handling quality**: typed errors, no panic paths in library behavior, clear failure propagation.
7. **Async/concurrency correctness**: non-blocking runtime behavior, cancellation/timeout behavior, channel/oneshot handoff clarity.
8. **Testability & verification depth**: acceptance criteria are deterministic, observable, and include regression-prone branches.
9. **Cross-SDK developer ergonomics**: payload compatibility + naming stability to prevent consumer-side breakage.

## Consolidated findings

### 1) TypeScript event naming is still inconsistent across docs vs examples
- Severity: Medium
- Dimensions: (1) Contract consistency, (9) Cross-SDK ergonomics
- Detail:
  - v0.1 TypeScript issue docs describe camelCase event discriminants.
  - Other v0.1 acceptance/event references and examples still use snake_case naming.
- Risk:
  - Consumers may implement brittle switch/case logic against a mismatched wire format.
  - Typings can drift from actual runtime payloads.
- Recommendation:
  - Freeze one canonical wire format in v0.1 (recommended: snake_case, aligned with cross-language runtime event schema), then add TS-side alias/helper mapping if needed.

### 2) Thinking boundary chunks are underspecified at adapter acceptance level
- Severity: Medium
- Dimensions: (2) Streaming protocol completeness, (8) Testability
- Detail:
  - Core type design includes `ThinkingStart`/`ThinkingEnd` to preserve stream boundaries.
  - Adapter acceptance focuses on delta events but does not force explicit boundary emission behavior.
- Risk:
  - Different adapters/providers may produce incompatible event streams for thinking blocks.
  - UI/TUI/log renderers lose deterministic folding boundaries.
- Recommendation:
  - Add acceptance checks that boundary chunks are emitted whenever upstream provider events expose start/end semantics.

### 3) Approval flow E2E misses denied-path verification
- Severity: Low
- Dimensions: (3) Control-path coverage, (8) Testability
- Detail:
  - E2E validation checks `approval_requested` and `approval_granted`, but not a required denied-path assertion.
- Risk:
  - Rejection handling can regress unnoticed (e.g., wrong status transition, incorrect follow-up loop behavior).
- Recommendation:
  - Add one deterministic E2E scenario that triggers `approval_denied`, verifies continuation behavior, and checks final run status semantics.

### 4) `read_file` security posture needs explicit “known risk” cross-link in E2E acceptance
- Severity: Low
- Dimensions: (4) Security boundary clarity, (8) Testability
- Detail:
  - v0.1 intentionally keeps permissive read_file behavior (no allowlist), with tightening deferred.
  - E2E requirements do not explicitly confirm operators can observe and reason about this risk boundary at runtime.
- Risk:
  - Teams may treat v0.1 behavior as production-safe by default.
- Recommendation:
  - In E2E checklist, add a note/assertion that this is an accepted v0.1 limitation and must be documented in demo output/readme guidance.

## Code-quality assessment snapshot

- **Modularity & SoC**: ✅ Overall aligned with architecture intent (core vs bindings separation is明确 in iteration decomposition), but should be enforced by acceptance wording in SDK issues.
- **Error handling quality**: ⚠️ Partially specified; several issues define error behavior, but cross-issue consistency checks for error-code shape can be strengthened.
- **Async/concurrency correctness**: ⚠️ Major paths are present (run loop, async job, approval wait), but cancellation/timeout race expectations should be made more explicit in E2E assertions.
- **Testability depth**: ⚠️ Baseline good; denied-approval and security-limit observability are the two main gaps.
- **Cross-SDK ergonomics**: ⚠️ Event naming inconsistency is the highest practical breakage risk.

## Overall assessment
- The iteration is coherent and implementation-complete at milestone level.
- Main remaining work is **spec/contract alignment hardening + code-quality acceptance tightening** across event naming, streaming boundary semantics, and E2E control-path coverage.
- Addressing the four findings above will materially reduce downstream breakage in SDK consumers and future v0.2 migration.
