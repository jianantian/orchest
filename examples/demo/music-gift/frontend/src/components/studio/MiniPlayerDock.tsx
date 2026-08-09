import { useEffect, useState } from "react";
import { useI18n } from "../../i18n";
import { ChevronUpIcon, PauseIcon, PlayIcon } from "../Icons";

export interface MiniPlayerDockProps {
  title?: string;
  /** e.g. "V2 · 最新" — appended to the title line when present. */
  versionLabel?: string;
  coverUrl: string | null;
  /** The shared audio element living inside the sheet's PlayerCard — the
   *  dock never creates a second one; it only mirrors state and toggles
   *  playback through this element (null = not mounted yet, play disabled). */
  audio: HTMLAudioElement | null;
  currentTime: number;
  duration: number;
  onExpand: () => void;
}

/** Mobile bottom-docked mini player (Task 11): glass bar with disc, title,
 *  a hairline progress bar, play/pause passthrough, and the ⌃ handle that
 *  opens the listen sheet. Rendered only when there is an audioUrl. */
export function MiniPlayerDock({ title, versionLabel, coverUrl, audio, currentTime, duration, onExpand }: MiniPlayerDockProps) {
  const { t } = useI18n();
  const [playing, setPlaying] = useState(false);

  // Mirror the element's play state — the toggle can also come from the
  // full AudioPlayer inside the sheet, so events are the source of truth.
  useEffect(() => {
    if (!audio) { setPlaying(false); return; }
    const sync = () => setPlaying(!audio.paused);
    sync();
    audio.addEventListener("play", sync);
    audio.addEventListener("pause", sync);
    return () => {
      audio.removeEventListener("play", sync);
      audio.removeEventListener("pause", sync);
    };
  }, [audio]);

  function toggle() {
    if (!audio) return;
    if (audio.paused) void audio.play();
    else audio.pause();
  }

  const pct = duration > 0 ? Math.min(100, (currentTime / duration) * 100) : 0;
  const label = [title, versionLabel].filter(Boolean).join(" · ");

  return (
    <div className="wb-dock">
      <div
        className="wb-disc"
        style={coverUrl ? { backgroundImage: `url(${coverUrl})`, backgroundSize: "cover", backgroundPosition: "center" } : undefined}
      />
      <div className="wb-dock-meta">
        {label && <p className="wb-dock-title">{label}</p>}
        <div className="wb-dock-progress"><i style={{ width: `${pct}%` }} /></div>
      </div>
      <button type="button" className="wb-icon-btn" onClick={toggle} disabled={!audio} aria-label={playing ? "Pause" : "Play"}>
        {playing ? <PauseIcon /> : <PlayIcon />}
      </button>
      <button type="button" className="wb-icon-btn" onClick={onExpand} title={t("dock_expand")} aria-label={t("dock_expand")}>
        <ChevronUpIcon />
      </button>
    </div>
  );
}
