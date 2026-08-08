import { useRef, useState, type PointerEvent } from "react";

export interface RangeValue {
  start: number;
  end: number;
}

export interface RangeSelectProps {
  /** Track length in seconds; the caller hides the bar until this is > 0. */
  duration: number;
  value: RangeValue | null;
  onChange: (v: RangeValue | null) => void;
}

type Handle = "start" | "end";

/** Active drag: which handle moves, the other handle's pinned position
 *  (`anchor`), and the pointer's raw unclamped seconds. */
interface Drag {
  which: Handle;
  anchor: number;
  sec: number;
}

/** Rubber-band decay for an overshoot of `x` seconds past a boundary of a
 *  `d`-second track — visual displacement only; the committed value clamps
 *  back into [0, duration] on release. */
function rubber(x: number, d: number): number {
  return (x * d * 0.55) / (d + 0.55 * Math.abs(x));
}

/** 播放器进度条上的选段覆盖层：track + 双 handle + 选段高亮带。
 *  pointerdown + setPointerCapture 1:1 跟踪；拖过 0/duration 时视觉位移按
 *  橡皮筋公式衰减，松手 clamp 回 [0, duration] 后经 onChange 提交秒值。
 *  纯受控组件 —— 秒值状态与 LRC 映射都在 Studio。 */
export function RangeSelect({ duration, value, onChange }: RangeSelectProps) {
  const barRef = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);

  function secAt(clientX: number): number {
    const rect = barRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0 || duration <= 0) return 0;
    return ((clientX - rect.left) / rect.width) * duration;
  }

  /** Raw drag seconds → visual seconds: rubber-band decay outside the
   *  track, pinned to the anchored handle so handles never cross. */
  function visualSec(d: Drag): number {
    const raw = d.sec < 0
      ? rubber(d.sec, duration)
      : d.sec > duration
        ? duration + rubber(d.sec - duration, duration)
        : d.sec;
    return d.which === "start" ? Math.min(raw, d.anchor) : Math.max(raw, d.anchor);
  }

  function onPointerMove(e: PointerEvent<HTMLDivElement>) {
    if (!drag) return;
    setDrag({ ...drag, sec: secAt(e.clientX) });
  }

  function onPointerUp() {
    if (!drag) return;
    const sec = Math.min(Math.max(drag.sec, 0), duration);
    onChange(drag.which === "start"
      ? { start: Math.min(sec, drag.anchor), end: drag.anchor }
      : { start: drag.anchor, end: Math.max(sec, drag.anchor) });
    setDrag(null);
  }

  function beginHandleDrag(which: Handle, e: PointerEvent<HTMLDivElement>) {
    if (duration <= 0) return;
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({
      which,
      anchor: which === "start" ? value?.end ?? 0 : value?.start ?? 0,
      sec: secAt(e.clientX),
    });
  }

  /** A press on the bare track starts a fresh zero-width selection anchored
   *  there; the press seamlessly becomes a drag of the end handle. */
  function onTrackDown(e: PointerEvent<HTMLDivElement>) {
    if (duration <= 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    const sec = Math.min(Math.max(secAt(e.clientX), 0), duration);
    onChange({ start: sec, end: sec });
    setDrag({ which: "end", anchor: sec, sec });
  }

  const start = drag?.which === "start" ? visualSec(drag) : value?.start ?? 0;
  const end = drag?.which === "end" ? visualSec(drag) : value?.end ?? 0;
  const hasSel = value !== null || drag !== null;
  const pct = (s: number) => `${duration > 0 ? (s / duration) * 100 : 0}%`;

  return (
    <div className="wb-range-bar" ref={barRef} onPointerDown={onTrackDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp}>
      <div className="wb-range-track" />
      {hasSel && (
        <div className="wb-range-sel" style={{ left: pct(start), width: pct(Math.max(end - start, 0)) }} />
      )}
      {hasSel && (
        <div
          className="wb-range-handle l"
          style={{ left: pct(start) }}
          onPointerDown={e => beginHandleDrag("start", e)}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        />
      )}
      {hasSel && (
        <div
          className="wb-range-handle r"
          style={{ left: `calc(${pct(end)} - 4px)` }}
          onPointerDown={e => beginHandleDrag("end", e)}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        />
      )}
    </div>
  );
}
