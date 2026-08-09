import type { ReactNode } from "react";
import { useI18n } from "../../i18n";
import AudioPlayer from "../AudioPlayer";

export interface PlayerCardProps {
  /** Null when the gift has no audio yet — the card body stays empty then. */
  audioUrl: string | null;
  /** Cover art for the disc; null shows the plain vinyl gradient. */
  coverUrl: string | null;
  title?: string;
  /** e.g. "V2 · 最新"; empty when the version list never loaded. */
  versionLabel: string;
  onTimeUpdate: (t: number) => void;
  /** Fires with the audio length once metadata loads (passthrough of
   *  AudioPlayer's onDuration — no DOM queries). */
  onDuration?: (duration: number) => void;
  /** Range-selection bar rendered right under the player (Task 9); hidden
   *  while generating, same as the player itself. */
  rangeBar?: ReactNode;
  /** Passthrough of AudioPlayer's registerAudio (Task 11): the mobile mini
   *  player dock toggles playback through this element — no second audio. */
  registerAudio?: (el: HTMLAudioElement | null) => void;
  /** True while a regeneration is in flight — the old audio is gone
   *  server-side, so the card shows the generating status instead of a
   *  broken player (same semantics as the old edit-head). */
  generating?: boolean;
  /** Lyrics/LRC area rendered under the player (hidden while generating). */
  children?: ReactNode;
}

/** Artifact-column card: 试听卡 — cover disc + title/version row, the
 *  existing AudioPlayer, an optional range-selection bar (Task 9), and the
 *  LRC/plain-lyrics panel passed in as children. Pure presentational move
 *  out of Studio's edit-head; all state stays in Studio. */
export function PlayerCard({ audioUrl, coverUrl, title, versionLabel, onTimeUpdate, onDuration, registerAudio, rangeBar, generating = false, children }: PlayerCardProps) {
  const { t } = useI18n();
  return (
    <div className="wb-card wb-player-card">
      {generating ? (
        <div className="polish-status"><span className="spinner" /> {t("generating")}</div>
      ) : audioUrl ? (
        <>
          <div className="wb-player">
            <div
              className="wb-disc"
              style={coverUrl ? { backgroundImage: `url(${coverUrl})`, backgroundSize: "cover", backgroundPosition: "center" } : undefined}
            />
            <div className="wb-player-meta">
              {title && <p className="wb-player-title">{title}</p>}
              {versionLabel && <p className="wb-player-sub">{versionLabel}</p>}
            </div>
          </div>
          <AudioPlayer key={audioUrl} src={audioUrl} title={title} onTimeUpdate={onTimeUpdate} onDuration={onDuration} registerAudio={registerAudio} />
          {rangeBar}
        </>
      ) : null}
      {!generating && children}
    </div>
  );
}
