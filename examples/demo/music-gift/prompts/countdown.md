You are a creative frontend engineer. Generate an embeddable birthday countdown HTML block for the person below. It will be inserted at the top of an existing page, with a music player and lyrics below.

【Person Info】
Name/nickname: {name}
Birthday: {month} {day} ({days_until} days away)
Countdown target date (MUST use this exact value): {target_date}
Personal scene: {scenario}

【Lyrics snippet (extract key imagery to strongly tie this block to their story)】
{lyric_snippet}

【Output format (strict)】
Output must be a self-contained HTML block with this structure:

<style>
  /* All styles here, all selectors must start with #birthday-section for namespace isolation */
  /* @import url() allowed for Google Fonts, no other external resources */
</style>

<div id="birthday-section">
  <!-- All content here -->
</div>

<script>
  (function() {
    /* All JS in IIFE to avoid global pollution */
    /* Use document.getElementById / querySelector to access elements in the div above */
    /* Don't use DOMContentLoaded — execute directly (elements are already in DOM) */
  })();
</script>

Forbidden: <!DOCTYPE>, <html>, <head>, <body> tags.
Output only the three blocks above, no explanatory text.

【Design requirements】
1. Background: match the outer page #f7f3ec or pick a warm coordinating tone; padding 24-32px
2. 1-2 accent colors drawn from the person's story; warm hand-drawn illustration style
3. 2-3 simple SVG line illustrations (viewBox="0 0 100 100"):
   - Stroke only, no fill; stroke-linecap:round; stroke-linejoin:round; stroke-width 2-3px
   - Must relate to the person's specific story — no generic cakes or balloons
4. Live countdown to the second via setInterval, showing days/hours/minutes/seconds in large type
   【CRITICAL】Countdown target must use the date string "{target_date}" exactly: new Date('{target_date}'). Do not calculate the year yourself.
5. 1-2 delightful interactions (must use addEventListener), themed to the story:
   - walking scene → click ground to leave footprints; music scene → click note to make it bounce; etc.
6. The person's name + one personal line drawn from the lyrics/scene (max 30 chars, avoid clichés like "wishing you...")
7. Bottom padding = 0 — music player card sits directly below, seamless join{previous_error}
