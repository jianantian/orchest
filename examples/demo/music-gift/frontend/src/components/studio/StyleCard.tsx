import { useI18n } from "../../i18n";
import { isImeComposing } from "../../lib/ime";
import { MusicNoteIcon, XIcon, FemaleIcon, MaleIcon, MicIcon } from "../Icons";

export interface StyleCardProps {
  selectedStyles: string[];
  styleInput: string;
  onStyleInputChange: (v: string) => void;
  styleSuggestions: string[];
  instrumental: boolean;
  vocalGender: "female" | "male" | null;
  onInstrumentalChange: (v: boolean) => void;
  onVocalGenderChange: (v: "female" | "male" | null) => void;
  onAddStyle: (s: string) => void;
  onRemoveStyle: (s: string) => void;
  onCommitStyleInput: () => void;
  onRefreshSuggestions: () => void;
  /** " ai-flash" while the AI-applied style/vocal change is flashing. */
  styleFlash: string;
  vocalFlash: string;
}

/** Driver-column card: selected style chips + inline "+ 风格" input + the
 * persistent suggestion row (6 pills + ↻), merged with the two vocal
 * option rows (演唱方式 / 人声). Pure presentational move out of Studio —
 * all state and handlers are injected, semantics unchanged. The old
 * DraftSection collapse is retired: the card is always expanded. */
export function StyleCard({
  selectedStyles,
  styleInput,
  onStyleInputChange,
  styleSuggestions,
  instrumental,
  vocalGender,
  onInstrumentalChange,
  onVocalGenderChange,
  onAddStyle,
  onRemoveStyle,
  onCommitStyleInput,
  onRefreshSuggestions,
  styleFlash,
  vocalFlash,
}: StyleCardProps) {
  const { t } = useI18n();
  return (
    <div className="wb-card wb-style-card">
      {/* Style section — chips + inline input + persistent suggestion row. */}
      <div className={`wb-style-section${styleFlash}`}>
        <div className="wb-card-head"><span className="studio-ai-title">{t("free_style")}</span></div>
        <div className="style-edit-row">
          {selectedStyles.length > 0 && (
            <div className="style-chips">{selectedStyles.map(s => <span key={s} className="style-chip" onClick={() => onRemoveStyle(s)} role="button" tabIndex={0} onKeyDown={e => e.key === "Enter" && onRemoveStyle(s)}>{s} <XIcon /></span>)}</div>
          )}
          <input type="text" className="style-input" value={styleInput} onChange={e => onStyleInputChange(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter" && !isImeComposing(e)) { e.preventDefault(); onCommitStyleInput(); } else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0) onRemoveStyle(selectedStyles[selectedStyles.length - 1]); }}
            placeholder={t("add_style_ph")} />
        </div>
        <div className="style-suggestions">
          {styleSuggestions.map(s => <button key={s} type="button" className="opt-pill opt-pill-sm" onClick={() => onAddStyle(s)}>{s}</button>)}
          <button type="button" className="icon-btn" onClick={onRefreshSuggestions} aria-label="↻">↻</button>
        </div>
      </div>

      {/* Vocal options — 演唱方式 [演唱|器乐] and 人声 [女声|男声]
          (disabled under 器乐). */}
      <div className={`wb-vocal-section${vocalFlash}`}>
        <div className="opt-row">
          <span className="opt-row-label">{t("vocal_mode")}</span>
          <button type="button" className={`opt-pill opt-pill-sm${!instrumental ? " on" : ""}`} onClick={() => onInstrumentalChange(false)}>
            <MicIcon /> {t("vocal_sung")}
          </button>
          <button type="button" className={`opt-pill opt-pill-sm${instrumental ? " on" : ""}`} onClick={() => onInstrumentalChange(true)}>
            <MusicNoteIcon /> {t("instrumental")}
          </button>
        </div>
        <div className="opt-row">
          <span className="opt-row-label">{t("vocal")}</span>
          <button type="button" className={`opt-pill opt-pill-sm${!instrumental && vocalGender === "female" ? " on" : ""}`} disabled={instrumental}
            onClick={() => { const deselect = vocalGender === "female"; onInstrumentalChange(false); onVocalGenderChange(deselect ? null : "female"); }}>
            <FemaleIcon /> {t("gender_female")}
          </button>
          <button type="button" className={`opt-pill opt-pill-sm${!instrumental && vocalGender === "male" ? " on" : ""}`} disabled={instrumental}
            onClick={() => { const deselect = vocalGender === "male"; onInstrumentalChange(false); onVocalGenderChange(deselect ? null : "male"); }}>
            <MaleIcon /> {t("gender_male")}
          </button>
        </div>
      </div>
    </div>
  );
}
