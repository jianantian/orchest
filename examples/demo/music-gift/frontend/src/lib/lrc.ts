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

/** 把音频时间选段 [startSec,endSec) 映射到覆盖的 LRC 行范围（1-based 闭区间）。
 *  第 i 行覆盖 [lines[i].time, lines[i+1].time)。选段全在首行之前或 lines 为空 → null。 */
export function linesForRange(lines: LRCLine[], startSec: number, endSec: number): { from: number; to: number } | null {
  if (lines.length === 0 || endSec <= lines[0].time) return null;
  let from = -1, to = -1;
  for (let i = 0; i < lines.length; i++) {
    const lineStart = lines[i].time;
    const lineEnd = i + 1 < lines.length ? lines[i + 1].time : Infinity;
    if (lineStart < endSec && startSec < lineEnd) {
      if (from === -1) from = i + 1;
      to = i + 1;
    }
  }
  return from === -1 ? null : { from, to };
}
