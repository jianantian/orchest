# 002 — Resolved Execution Config and Attempt Lifecycle

## Background

The v0.16 runner fingerprints a synthetic three-Tool registry and hard-codes
empty request options, fake capability routes, and no session persistence. Those
values can differ from the product pipeline that actually runs. Attempt records
also collapse missing terminals into generic failures, omit the terminal stop
reason, tolerate cleanup failure, and ignore artifact-write errors.

## Goal and scope

Create one application-level resolved execution plan used by both product/eval
execution and effective-config snapshotting, then make attempt status reflect
the retained runtime evidence conservatively.

## Acceptance criteria

- [ ] Product `run`/`resume` and eval execution derive AgentConfig, reviewer
      config, Tool registry, capability routes, request options, and session
      behavior from the same typed resolved inputs.
- [ ] Effective-config schema records stable case profiles containing the exact
      Tool fingerprints and fresh/follow-up persistence modes used by selected
      cases.
- [ ] Fingerprints include reviewer, audio, image, report, and synthesis Tools
      whenever enabled, with the same schemas/metadata as execution.
- [ ] Live/injected chat identity, resolved max tokens, credential-free endpoint,
      ASR/TTS/vision route, retry/budget/approval/supervision, hooks/store labels,
      and other behavior-affecting non-secret values are captured before the
      first model call.
- [ ] Harness prompt/description text is excluded from effective config and
      represented only by stable surface IDs.
- [ ] Credential-bearing endpoint userinfo/query, secret-like option fields, or
      an enabled component without a stable label fails before run-directory
      creation and before model calls.
- [ ] Changing actual max tokens, endpoint, capability route, Tool schema,
      runtime option, or session profile changes the effective-config hash;
      changing only harness text does not.
- [ ] `EventsDropped`, a started stream without terminal, early stream closure,
      or failed required store cleanup produces `inconclusive`.
- [ ] A retained `RunFailed` terminal or pre-run provider/model failure produces
      `execution_failure`; `RunCompleted` with required cleanup succeeds as
      `completed`.
- [ ] `attempt.json` records the actual terminal kind and stop reason.
- [ ] All completed, execution-failure, and inconclusive attempts write the four
      fixed files; an artifact-write error fails the eval loudly instead of
      logging and continuing.
- [ ] Offline runner tests cover success, provider failure, event drop, missing
      terminal, early closure, cleanup failure, label conflict, and four-file
      output.
- [ ] No code under `crates/` or bindings changes.

## Implementation plan

1. Add failing snapshot tests for the actual Tool/capability/session/request
   inputs and secret endpoint preflight.
2. Extract a typed resolved Briefing Desk execution plan and route product/eval
   builders through it.
3. Version the effective-config schema forward and generate profiles from those
   resolved inputs before any call.
4. Add failing attempt-classification tests and implement an evidence-based
   classifier with explicit terminal/cleanup precedence.
5. Propagate artifact-write errors and add runner-level failure-path tests.
6. Run focused tests, clippy/fmt, self-review, and commit once.
