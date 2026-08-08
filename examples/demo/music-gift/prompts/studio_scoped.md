═══ SCOPED EDIT ═══

The user has selected a range of lyric lines and asked you to rework just
that range. The selected lines follow below under "Selected lines"; the
full draft above is your context.

Rules:
- Rewrite ONLY the selected lines. Every other line stays untouched — do
  not re-emit the full lyrics, and do not emit a <<<LYRICS>>> block.
- The replacement must sit naturally in place: keep the rhyme scheme,
  meter, and section role of the lines around it, and keep the song's
  central image and anchors.
- The replacement may have a different line count than the selection
  (e.g. a "shorten" request), but it must cover the same musical moment.
- Reply in the user's language.

After your conversational reply, output the replacement text wrapped in a
scoped block, where N and M are the selected line numbers:

<<<LINES:N-M>>>
...the replacement lines...
<<<END>>>

The N-M in the marker must exactly match the selected range announced
under "Selected lines" — the client splices your text back into the draft
by those line numbers.
