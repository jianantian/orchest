import { useEffect, useRef, useState, type CSSProperties } from "react";
import { type LRCLine } from "../lib/lrc";

export interface LRCViewerProps {
  lines: LRCLine[];
  currentTime: number;
  onSeek?: (time: number) => void;
}

const LINE_HEIGHT = 36;
const VISIBLE_LINES = 7;

export function LRCViewer({ lines, currentTime, onSeek }: LRCViewerProps) {
  const [activeIndex, setActiveIndex] = useState(-1);
  const [userScroll, setUserScroll] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const scrollTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  /** Set while a programmatic follow-along scroll is animating. */
  const isAutoScrollRef = useRef(false);
  const autoScrollTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    if (lines.length === 0) return;
    let idx = -1;
    for (let i = lines.length - 1; i >= 0; i--) {
      if (lines[i].time <= currentTime) {
        idx = i;
        break;
      }
    }
    if (idx !== activeIndex) setActiveIndex(idx);
  }, [currentTime, lines, activeIndex]);

  useEffect(() => {
    const el = containerRef.current;
    if (userScroll || activeIndex < 0 || !el) return;
    const top = Math.max(0, activeIndex * LINE_HEIGHT - (VISIBLE_LINES * LINE_HEIGHT) / 2 + LINE_HEIGHT / 2);
    if (el.scrollTop === top) return;
    // A smooth scroll fires onScroll on every animation frame. Mark it as
    // programmatic so handleScroll doesn't read our own follow-along as a
    // user override (which used to suppress auto-scroll for 3s after every
    // line). The flag clears once scroll events stop arriving — i.e. the
    // animation has settled — so a real user scroll afterwards registers.
    isAutoScrollRef.current = true;
    el.scrollTo({ top, behavior: "smooth" });
  }, [activeIndex, userScroll]);

  function handleScroll() {
    if (isAutoScrollRef.current) {
      clearTimeout(autoScrollTimerRef.current);
      autoScrollTimerRef.current = setTimeout(() => { isAutoScrollRef.current = false; }, 150);
      return;
    }
    setUserScroll(true);
    clearTimeout(scrollTimerRef.current);
    scrollTimerRef.current = setTimeout(() => setUserScroll(false), 3000);
  }

  if (lines.length === 0) return null;

  return (
    <div className="lrc-container" ref={containerRef} onScroll={handleScroll}>
      <div className="lrc-scroller" style={{ "--lrc-h": `${lines.length * LINE_HEIGHT}px` } as CSSProperties}>
        {lines.map((line, i) => (
          <div
            key={i}
            className={`lrc-line ${i === activeIndex ? "active" : ""}`}
            onClick={() => onSeek?.(line.time)}
            role="button"
            tabIndex={0}
          >
            {line.text}
          </div>
        ))}
      </div>
    </div>
  );
}
