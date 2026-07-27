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
Wrap the three blocks in ONE ```html fenced code block — no explanatory text before or after the fence.

【Hard limits】
- The block MUST end with the closing </script> tag (the closing fence follows immediately after). A truncated block (unclosed <script>) is a hard failure — it kills the countdown timer and every interaction.
- Length budget: keep the whole block compact (roughly ≤ 350 lines). At most 3 small SVGs, a few short keyframes, no long keyframe libraries. If space runs short, cut decoration — never cut structure.

【Design requirements】
1. Background: match the outer page #f7f3ec or pick a warm coordinating tone; padding 24-32px
2. 1-2 accent colors drawn from the person's story; warm hand-drawn illustration style
3. 2-3 simple SVG line illustrations (viewBox="0 0 100 100"):
   - Stroke only, no fill; stroke-linecap:round; stroke-linejoin:round; stroke-width 2-3px
   - Must relate to the person's specific story — no generic cakes or balloons
4. Live countdown to the second via setInterval, showing days/hours/minutes/seconds in large type
   【CRITICAL】The countdown must hit zero at LOCAL midnight of {target_date}. Build the target from its date parts:
   const [y, m, d] = '{target_date}'.split('-').map(Number); const target = new Date(y, m - 1, d);
   NEVER use new Date('{target_date}') — a bare date string parses as UTC midnight, so the countdown ends hours early for users east of UTC. Do not calculate the year yourself.
5. 1-2 delightful interactions (must use addEventListener), themed to the story:
   - walking scene → click ground to leave footprints; music scene → click note to make it bounce; etc.
6. The person's name + one personal line drawn from the lyrics/scene (max 30 chars, avoid clichés like "wishing you...")
7. Bottom padding = 0 — music player card sits directly below, seamless join
8. CJK-safe fonts: Latin display fonts (e.g. Baloo 2) have no Chinese glyphs — always append a CJK fallback stack so a Chinese name never drops to the raw system default:
   font-family: <display-font>, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans SC", sans-serif;
9. No emoji anywhere — every visual is inline SVG line art (emoji clash with the hand-drawn style)
10. Responsive floor: font sizes via clamp(), content rows via flex-wrap; nothing overflows horizontally at 360px width
{previous_error}
