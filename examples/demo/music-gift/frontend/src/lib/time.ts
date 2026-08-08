/** Format seconds as M:SS (floored), matching the audio player clock. */
export function fmtMSS(t: number): string {
  if (!t || isNaN(t) || t < 0) return "0:00";
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}
