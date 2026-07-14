import { useEffect, useRef, useState } from "react";

export type MusicCardState = "generating" | "ready" | "error";

export interface MusicCardProps {
  initialState?: MusicCardState;
  estimatedSec?: number;
  onOpen?: () => void;
  onRetry?: () => void;
}

export function MusicCard({
  initialState = "generating",
  estimatedSec = 60,
  onOpen,
  onRetry,
}: MusicCardProps) {
  const [state, setState] = useState<MusicCardState>(initialState);
  const [progress, setProgress] = useState(0);
  const startRef = useRef(Date.now());
  const ivRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);
  const fillerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (state !== "generating") return;

    startRef.current = Date.now();
    ivRef.current = setInterval(() => {
      const elapsed = (Date.now() - startRef.current) / 1000;
      const ratio = elapsed / estimatedSec;
      const pct = Math.min(90, Math.round((ratio / (ratio + 0.3)) * 100));
      setProgress(pct);
    }, 1000);

    return () => clearInterval(ivRef.current);
  }, [state, estimatedSec]);

  useEffect(() => {
    setState(initialState);
  }, [initialState]);

  useEffect(() => {
    if (fillerRef.current) {
      fillerRef.current.style.width = `${state === "ready" ? 100 : progress}%`;
    }
  }, [progress, state]);

  if (state === "generating") {
    return (
      <div className="music-card">
        <div className="music-card-inner">
          <div className="vinyl spinning" />
          <div className="mc-info">
            <div className="mc-title">Creating your song…</div>
            <div className="mc-sub">Estimated {estimatedSec}s</div>
          </div>
          <div className="waveform">
            <span /><span /><span /><span /><span />
          </div>
        </div>
        <div className="mc-progress">
          <div ref={fillerRef} className="mc-progress-fill" />
        </div>
      </div>
    );
  }

  if (state === "error") {
    return (
      <div className="music-card ready" onClick={onRetry} role="button" tabIndex={0}>
        <div className="music-card-inner">
          <div className="mc-art">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z" />
            </svg>
          </div>
          <div className="mc-info">
            <div className="mc-title">Generation failed</div>
            <div className="mc-sub">Tap to retry</div>
          </div>
        </div>
        <div className="mc-progress">
          <div className="mc-progress-fill" style={{ width: "100%" }} />
        </div>
      </div>
    );
  }

  return (
    <div className="music-card ready" onClick={onOpen} role="button" tabIndex={0}>
      <div className="music-card-inner">
        <div className="vinyl done" />
        <div className="mc-info">
          <div className="mc-title">Your song is ready!</div>
          <div className="mc-sub">Tap to open</div>
        </div>
        <div className="mc-arrow">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
        </div>
      </div>
      <div className="mc-progress">
        <div className="mc-progress-fill" style={{ width: "100%" }} />
      </div>
    </div>
  );
}
