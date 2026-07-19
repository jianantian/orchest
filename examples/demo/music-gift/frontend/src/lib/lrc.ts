export interface LRCLine {
  time: number;
  text: string;
}

/** Parse LRC text into timestamped lines. */
export function parseLRC(raw: string): LRCLine[] {
  const lines: LRCLine[] = [];
  for (const line of raw.split("\n")) {
    const match = line.match(/^\[(\d{2}):(\d{2}(?:\.\d+)?)\](.*)/);
    if (!match) continue;
    const mins = parseInt(match[1], 10);
    const secs = parseFloat(match[2]);
    const text = match[3].trim();
    if (text) lines.push({ time: mins * 60 + secs, text });
  }
  return lines;
}
