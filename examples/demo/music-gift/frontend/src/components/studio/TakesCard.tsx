import { useI18n } from "../../i18n";
import { BranchIcon, PlayIcon } from "../Icons";
import type { GiftVersion } from "../../types";

export interface TakesCardProps {
  /** Newest first; index 0 carries the "latest" label. */
  versions: GiftVersion[];
  currentIdx: number;
  /** ▶ 试听: switch the displayed audio/lyrics to this take. */
  onSelect: (i: number) => void;
  /** ⑂ 分叉: copy this take's work fields into the draft (loadVersionToDraft). */
  onBranch: (i: number) => void;
}

function fmtDuration(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/** Artifact-column card: 版本卡 — one divider-separated row per take,
 *  badge + title/style meta + icon actions. Pure presentational move of
 *  the old version-bar; selection/branch state stays in Studio. */
export function TakesCard({ versions, currentIdx, onSelect, onBranch }: TakesCardProps) {
  const { t } = useI18n();
  return (
    <div className="wb-card wb-takes-card">
      {versions.map((v, i) => {
        const sub = [v.meta.style, v.duration_secs ? fmtDuration(v.duration_secs) : null].filter(Boolean).join(" · ");
        return (
          <div key={v.version} className={`wb-take${i === currentIdx ? " on" : ""}`}>
            <span className="wb-take-badge">V{v.version}</span>
            <div className="wb-take-meta">
              <p className="wb-take-name">{v.meta.title ?? `V${v.version}`}{i === 0 ? ` · ${t("version_latest")}` : ""}</p>
              {sub && <p className="wb-take-sub">{sub}</p>}
            </div>
            <div className="wb-take-actions">
              {i !== currentIdx && (
                <button type="button" className="wb-icon-btn" onClick={() => onSelect(i)} title={`${t("versions")} V${v.version}`} aria-label={`${t("versions")} V${v.version}`}>
                  <PlayIcon />
                </button>
              )}
              <button type="button" className="wb-icon-btn" onClick={() => onBranch(i)} title={t("load_to_draft")} aria-label={t("load_to_draft")}>
                <BranchIcon />
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
