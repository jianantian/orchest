import { useEffect, useRef, useState } from "react";
import { type LRCLine, parseLRC } from "../lib/lrc";

export { type LRCLine, parseLRC };

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
    if (userScroll || activeIndex < 0 || !containerRef.current) return;
    const targetY = activeIndex * LINE_HEIGHT - (VISIBLE_LINES * LINE_HEIGHT) / 2 + LINE_HEIGHT / 2;
    containerRef.current.scrollTo({ top: Math.max(0, targetY), behavior: "smooth" });
  }, [activeIndex, userScroll]);

  function handleScroll() {
    setUserScroll(true);
    clearTimeout(scrollTimerRef.current);
    scrollTimerRef.current = setTimeout(() => setUserScroll(false), 3000);
  }

  if (lines.length === 0) return null;

  return (
    <div className="lrc-container" ref={containerRef} onScroll={handleScroll}>
      <div className="lrc-scroller" style={{ height: lines.length * LINE_HEIGHT }}>
        {lines.map((line, i) => (
          <div
            key={i}
            className={`lrc-line ${i === activeIndex ? "active" : ""}`}
            style={{ height: LINE_HEIGHT }}
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
