/** 歌词行工具。行号约定：1-based、闭区间（与 UI 文案"第 N–M 行"一致）。 */

export function splitLines(lyrics: string): string[] {
  return lyrics.split("\n");
}

/** 把 lyrics 的第 from–to 行（1-based 闭区间，越界 clamp）替换为 replacement 行数组。 */
export function spliceLines(lyrics: string, from: number, to: number, replacement: string[]): string {
  const lines = splitLines(lyrics);
  const f = Math.min(Math.max(from, 1), lines.length);
  const t = Math.min(Math.max(to, f), lines.length);
  lines.splice(f - 1, t - f + 1, ...replacement);
  return lines.join("\n");
}

/** 把 textarea 的 selectionStart/End 映射为覆盖的整行范围（1-based 闭区间）。
 *  selEnd 为选区结束光标位置：光标停在换行符之前（text[selEnd] === "\n"）时不会多吃下一行。 */
export function lineRangeForSelection(text: string, selStart: number, selEnd: number): { from: number; to: number } {
  const from = text.slice(0, selStart).split("\n").length;
  // selEnd 落在换行符之前时，选区不含下一行
  const to = text.slice(0, Math.max(selEnd, selStart)).split("\n").length;
  return { from, to: Math.max(to, from) };
}

/** 可唱行判定：非空且不是 [段落] 标记行。 */
function isSingable(line: string): boolean {
  const t = line.trim();
  return t.length > 0 && !/^\[.*\]$/.test(t);
}

/** 把 LRC 行范围（1-based 闭区间，不含空行/标记行）映射回手稿行范围（含全部行）。
 *  序数语义：LRC 第 k 行 = 手稿第 k 个可唱行。无可唱行 → null。 */
export function manuscriptRangeForLrcRange(lyrics: string, lrcRange: { from: number; to: number }): { from: number; to: number } | null {
  const singableIdx: number[] = [];
  splitLines(lyrics).forEach((line, i) => { if (isSingable(line)) singableIdx.push(i + 1); });
  if (singableIdx.length === 0) return null;
  const from = singableIdx[Math.min(Math.max(lrcRange.from, 1), singableIdx.length) - 1];
  const to = singableIdx[Math.min(Math.max(lrcRange.to, 1), singableIdx.length) - 1];
  return { from, to: Math.max(to, from) };
}
