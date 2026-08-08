# 001 — Corpus and Grader Contract Repair

## Background

The v0.16 corpus validator loses case context when typed deserialization rejects
an unknown enum value. Citation grading strips path components before checking
for traversal, modality grading can count failed Tool calls, follow-up grading
trusts caller metadata without observable resume evidence, and committed grader
fixtures are not executed by the grader tests.

## Goal and scope

Repair these deterministic input/grading contracts without changing the corpus
split, case contents, grader weights, or scoring formulas.

## Acceptance criteria

- [ ] Corpus loading validates raw `case_id`, `split`, and `tags` before typed
      deserialization; unknown tags and illegal splits report the real case ID
      and exact field.
- [ ] Negative tests cover unknown tag, illegal split, duplicate ID, unknown
      fixture, invalid weight, empty expectation, mutable seed, seed hash,
      scenario-family isolation, split counts, and validation-tag coverage.
- [ ] Citation grading rejects absolute paths, `..` traversal, and canonical
      targets outside the fixture root before reducing a citation to a basename.
- [ ] A formal Sources citation `../<existing-fixture-basename>` fails.
- [ ] Modality coverage requires a successful completed call for every required
      audio/image Tool; start, retry, failure, or cancellation alone does not
      satisfy coverage.
- [ ] Follow-up grounding requires matching seed metadata plus observable
      follow-up resume evidence; caller-supplied metadata alone fails.
- [ ] Every committed grader trajectory/output fixture is parsed and executed
      through its production grader with an asserted pass/fail/boundary result.
- [ ] The seven behavior tags and all existing aggregation formulas remain
      unchanged.
- [ ] `cargo test -p briefing-desk-demo eval::case`, grader tests, clippy, and
      fmt pass for the changed files.

## Implementation plan

1. Add failing raw-corpus diagnostic tests, then introduce a contextual
   raw-value validation pass before `CaseCorpus` deserialization.
2. Add same-basename traversal failure coverage, then preserve and validate the
   raw citation path before basename matching.
3. Add failed-media and metadata-only follow-up tests, then tighten lifecycle
   predicates used by those graders.
4. Replace fixture-presence assertions with a table that loads every committed
   fixture and runs the corresponding production grader.
5. Run focused tests, clippy/fmt, self-review, and commit once.
