
export const STYLE_CATALOG: string[] = [
  "pop", "rock", "rap", "electronic", "jazz", "classical", "folk", "r&b", "soul",
  "latin", "metal", "blues", "country", "punk",
  "romantic", "emotional", "melancholic", "upbeat", "energetic", "sentimental",
  "heartbreak", "reflective", "relaxing", "bittersweet", "nostalgic", "happy",
  "dark", "intense", "sad", "inspirational", "dramatic", "uplifting", "hopeful",
  "passionate", "peaceful", "dreamy", "fun", "moody", "positive", "empowering",
  "soothing", "haunting", "playful", "sorrow", "longing", "cheerful", "epic",
  "piano", "guitar", "strings", "synth", "orchestral", "drums", "bass", "violin",
  "saxophone", "flute", "cello", "acoustic", "vocal",
  "danceable", "high energy", "mellow", "fast-paced", "slow", "soft", "powerful",
  "smooth", "rhythmic", "minimal", "atmospheric", "chill", "meditative",
  "healing", "warm", "light", "deep", "lively", "gentle", "bright", "moving",
];

export function shuffleStyles(exclude: string[], count = 14): string[] {
  const a = STYLE_CATALOG.filter((s) => !exclude.includes(s));
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a.slice(0, count);
}

/**
 * Strip <<<MARKER>>> scaffolding from model output: marker-delimited blocks
 * (content included), then any orphaned lone markers.
 *
 * Marker set the backend can emit (see src/agent.rs `strip_meta_tags`, which
 * applies the same pair-then-lone removal server-side): <<<LYRICS>>>,
 * <<<END>>>, <<<STYLE>>>/<<<STYLE_END>>>, <<<TITLE>>>/<<<TITLE_END>>>,
 * <<<VOCAL>>>/<<<VOCAL_END>>>, <<<READY>>>. Does not trim — callers that need
 * it (e.g. lyric rendering) trim at the call site.
 */
export function stripMarkers(raw: string): string {
  return raw.replace(/<<<[A-Z_]+>>>[^<]*<<<[A-Z_]+>>>/g, "").replace(/<<<[A-Z_]+>>>/g, "");
}

/** Parse lyrics into sections by [Label] markers. */
export function parseLyrics(raw: string): Array<{ label: string; lines: string[] }> {
  const parts = raw.split(/\[([^\]]+)\]/).filter(Boolean);
  const sections: Array<{ label: string; content: string }> = [];
  for (let i = 0; i < parts.length; i += 2) {
    const label = parts[i].trim();
    const content = parts[i + 1]?.trim() || "";
    if (content) sections.push({ label, content });
  }
  // Merge consecutive choruses
  const merged: Array<{ label: string; content: string }> = [];
  for (const s of sections) {
    const last = merged[merged.length - 1];
    if (last && last.label === s.label && last.label.toLowerCase().includes("chorus")) {
      last.content += "\n\n" + s.content;
    } else {
      merged.push({ ...s });
    }
  }
  return merged.map((s) => ({ label: s.label, lines: s.content.split("\n").map((l) => l.trim()).filter(Boolean) }));
}
