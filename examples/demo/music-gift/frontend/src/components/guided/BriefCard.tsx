import { useEffect, useState } from "react";
import { useI18n } from "../../i18n";
import type { LyricsDraft, StepMeta } from "../../hooks/useGuidedState";

/**
 * 需求卡 — the guided flow's collected brief, one row per question. Purely
 * presentational: unanswered rows show a `--` placeholder and fill in as the
 * flow advances (mood/vocal only exist once the first lyrics draft lands).
 * Rows fade in on mount with a 60ms stagger (inline transition-delay per
 * row; the reduced-motion override kills the transition, delay included).
 * Until Task 14 builds the right artifact column this renders at the top of
 * the guided chat stream.
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
  const rows: Array<{ label: string; value: string }> = [
    { label: t("brief_to"), value: to },
    { label: t("brief_occasion"), value: meta.scenarioLabel },
    { label: t("brief_mood"), value: draft?.style ?? "" },
    // Same display mapping as ReviewCard's vocal toggle: the stored value is
    // "female"/"male", the label reuses the gender_* keys.
    { label: t("brief_vocal"), value: draft ? t(draft.vocal === "male" ? "gender_male" : "gender_female") : "" },
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
