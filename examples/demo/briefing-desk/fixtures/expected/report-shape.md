# Expected Report Shape

This defines the required sections for the Markdown brief Briefing Desk writes in
response to the fixture question:

> "How is Loom's retention and competitive position looking this quarter — is it
> worth continuing to invest in for Q4?"

A generated brief does not need to match this document word-for-word. It must
contain every section below, and every claim tagged **[checkable]** must be
factually consistent with the fixture corpus. Section order may vary; content may
not be omitted.

## 1. Title and restated question

A title line and a one-sentence restatement of the user's question, so the reader
can confirm the agent understood what was asked.

## 2. Executive summary

3-5 sentences giving a direct answer (invest / hold / more data needed) before the
supporting detail. This is the section a reader skims first.

## 3. Key findings

Bulleted findings, each with an inline source citation. Findings must include, at
minimum:

- **[checkable]** Referral is the strongest and only channel trending up across
  Q1-Q3 (source: `001-retention-dashboard-notes.md`, corroborated by
  `chart.png` — cite the chart explicitly, e.g. `[chart.png, via vision]`).
- **[checkable]** Paid channel retention is trending down (source:
  `001-retention-dashboard-notes.md`).
- **[checkable]** Onboarding friction is the top support ticket theme at 41% of Q3
  volume (source: `002-support-ticket-summary.md`).
- **[checkable]** The customer interview quote about what happens if Loom
  disappeared must be the **verbatim line from the audio**, not the paraphrase in
  the follow-up notes, and must be cited as coming from the recording, e.g.
  `[interview.wav, via ASR]`. The paraphrase in `005-interview-followup-notes.md`
  ("took a couple of tries") is not a substitute and should not be presented as the
  quote.

## 4. Conflicts and data gaps

A dedicated section — not buried in the findings — that explicitly names:

- **[checkable]** The referral-channel Q3 retention conflict: 42% (finance-reconciled
  cut, `001-retention-dashboard-notes.md`, matches `chart.png`) vs. 35%
  (unreconciled raw funnel pull, `002-support-ticket-summary.md`). The brief must
  state both numbers, name both sources, and should surface the analyst's own
  caveat in `001` that the two cuts may disagree — it must not silently pick one
  number and drop the other.
- **[checkable]** Tempo's pricing is unknown/TBD pending their pricing page
  relaunch (`003-competitor-scan.md`) — the brief must not invent a number for it.

## 5. Recommendation

A recommendation that references the findings above (e.g. "worth continuing to
invest, with onboarding fixes prioritized over the Q4 renewal push, because X").
It is fine for the brief to say the retention conflict should be resolved before a
final Q4 budget call — that is a legitimate recommendation, not a missing answer.

## 6. Sources

A list of every fixture file cited above, so a reader can trace each finding back
to its source file.

## Optional: audio brief

If TTS synthesis was requested (not denied/skipped), the brief may note that an
audio version was produced and where it was written. This is not required content
for the Markdown report itself — see `validation-rubric.md` for how the TTS path is
checked separately.
