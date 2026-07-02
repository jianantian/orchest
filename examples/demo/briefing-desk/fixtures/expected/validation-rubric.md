# Validation Rubric

While building and running Briefing Desk (issues 002-006), classify every finding
into exactly one of the five categories below before recording it in the
validation report (issue 006). This rubric exists so the triage rule in
`docs/archive/iteration/v0_10/prd.md` ("Validation Triage Rule") has a consistent basis to
sort findings into demo blocker / release blocker / post-1.0 backlog.

## API friction

The core runtime's public surface (model adapter, `ToolRegistry`, approval,
event stream, `SessionStore`, sub-agent/handoff) is hard, surprising, or
error-prone to use correctly from application code — independent of any specific
provider.

*Judge by*: would a competent developer, reading only the public docs, plausibly
get this wrong or need a workaround? If yes, it's API friction.

*Example judgment*: "Registering a tool that both mutates state and requires
approval needed three undocumented steps in the right order" → API friction.
"I misread the docs and it was actually one call" → not a finding.

## Modality gateway friction

Same as API friction, but scoped specifically to the ASR / TTS / multimodal-image
/ AIGC gateway surfaces (`Asr`, `Tts`, `VoiceManager`, `ContentBlock::Image`,
`orchest-provider-visual`) — construction, fake-provider availability, asset
handling, or event-surface confusion for these gateways specifically.

*Judge by*: does the friction disappear if you swap the provider for a different
one behind the same trait, or is it specific to how the gateway itself is wired?
If the trait/registry shape itself is the problem, it's modality gateway friction.

*Example judgment*: "No reusable fake `Tts` impl exists outside a private test
module, so the offline smoke path needed one written from scratch" → modality
gateway friction. "The specific Volcengine adapter returned a malformed asset URL"
→ provider bug, out of scope for this rubric (per PRD, adapter-level bugs stay in
that crate's own tests).

## Documentation friction

The public docs (README, guide, rustdoc, quickstart) are missing, wrong, or
insufficiently precise to build a feature without reading runtime source.

*Judge by*: did you have to read `crates/orchest/src/...` (not just the public
docs) to figure out how to call something that is meant to be public API?

*Example judgment*: "The quickstart shows registering a tool but not how approval
gating interacts with it" → documentation friction.

## Runtime bug

The runtime behaves incorrectly relative to its own documented/intended contract
— not a matter of the API being confusing, but of it doing the wrong thing.

*Judge by*: given correct usage per the docs, does the runtime produce an
incorrect, inconsistent, or panicking result?

*Example judgment*: "Denying an approval request still left a partial file on
disk" → runtime bug. "Session resume dropped the second-to-last message" →
runtime bug.

## Product bug

The Briefing Desk demo app itself (not the Orchest runtime or a provider gateway)
behaves incorrectly — a bug in `examples/demo/briefing-desk` application code.

*Judge by*: would fixing this require changing files under `examples/demo/`, not
under `crates/`?

*Example judgment*: "The demo's own markdown-writing tool double-escaped
citations" → product bug.

## Cross-cutting note

A single observation can only be filed under one category — pick the most
specific one that applies (modality gateway friction beats API friction if the
finding is gateway-specific; runtime bug beats API friction if the behavior is
simply wrong rather than merely confusing). If a finding is ambiguous between two
categories, record both candidate categories in the validation report and let the
triage rule (demo blocker / release blocker / post-1.0 backlog) resolve it — the
category label matters less than getting the finding recorded and triaged.
