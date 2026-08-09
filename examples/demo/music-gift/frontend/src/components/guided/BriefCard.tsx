import { useEffect, useState } from "react";
import { useI18n } from "../../i18n";
import type { LyricsDraft, StepMeta } from "../../hooks/useGuidedState";

/**
 * 需求卡 — the guided flow's collected brief, one row per question. Purely
 * presentational: unanswered rows show a `--` placeholder and fill in as the
 * flow advances (mood/vocal only exist once the first lyrics draft lands).
 * Rows fade in on mount with a 60ms stagger (inline transition-delay per
 * row; the reduced-motion override kills the transition, delay included).
 * Lives in the guided flow's right artifact column (Task 14), above the
 * step-dependent ReviewCard / MusicCard.
 */
export function BriefCard({ meta, draft }: { meta: StepMeta; draft: LyricsDraft | null }) {
  const { t } = useI18n();
  const [on, setOn] = useState(false);
  useEffect(() => {
    const id = requestAnimationFrame(() => setOn(true));
    return () => cancelAnimationFrame(id);
  }, []);

  // Pets skip the name question — the relationship label alone is the "to".
  const to = meta.name ? `${meta.name} (${meta.relationshipLabel})` : meta.relationshipLabel;
  // Same display mapping as ReviewCard's vocal toggle: the stored value is
  // "female"/"male", the label reuses the gender_* keys. Anything else
  // (missing draft, unexpected value) falls back to the `--` placeholder.
  const vocal = !draft ? "" : draft.vocal === "female" ? t("gender_female") : draft.vocal === "male" ? t("gender_male") : "";
  const rows: Array<{ label: string; value: string }> = [
    { label: t("brief_to"), value: to },
    { label: t("brief_occasion"), value: meta.scenarioLabel },
    { label: t("brief_mood"), value: draft?.style ?? "" },
    { label: t("brief_vocal"), value: vocal },
  ];

  return (
    <section className="wb-card wb-brief-card" aria-label={t("brief_title")}>
      <div className="wb-card-head"><span className="studio-ai-title">{t("brief_title")}</span></div>
      <dl className={`wb-brief${on ? " on" : ""}`}>
        {rows.map((r, i) => (
          <div key={r.label} className="wb-brief-row" style={{ transitionDelay: `${i * 60}ms` }}>
            <dt>{r.label}</dt>
            <dd className={r.value ? undefined : "empty"}>{r.value || "--"}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
