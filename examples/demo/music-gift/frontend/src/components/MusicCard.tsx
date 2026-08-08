import { useEffect, useRef, useState } from "react";
import { useI18n } from "../i18n";

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
  const { t } = useI18n();
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
            <div className="mc-title">{t("mc_creating")}</div>
            <div className="mc-sub">{t("mc_estimated", { n: estimatedSec })}</div>
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
            <div className="mc-title">{t("mc_failed")}</div>
            <div className="mc-sub">{t("mc_retry")}</div>
          </div>
        </div>
        <div className="mc-progress">
          <div className="mc-progress-fill full" />
        </div>
      </div>
    );
  }

  // Ready: the open affordance is a secondary button, not the whole card —
  // the card itself stays in place as a status artifact (in the workbench
  // the player takes over in the artifact column anyway).
  return (
    <div className="music-card ready">
      <div className="music-card-inner">
        <div className="vinyl done" />
        <div className="mc-info">
          <div className="mc-title">{t("mc_ready")}</div>
        </div>
        {onOpen && (
          <button type="button" className="btn btn-secondary" onClick={onOpen}>{t("mc_open")}</button>
        )}
      </div>
      <div className="mc-progress">
        <div className="mc-progress-fill full" />
      </div>
    </div>
  );
}
