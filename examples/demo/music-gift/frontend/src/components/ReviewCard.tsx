import { useState } from "react";
import { useI18n, type StyleTag } from "../i18n";

export interface ReviewData {
  lyrics: string;
  style: string;
  title: string;
  vocal: string;
}

export interface ReviewCardProps {
  lyrics: string;
  style: string;
  title: string;
  vocal: string;
  styleTags: StyleTag[];
  onSubmit: (data: ReviewData) => void;
  creating: boolean;
  /** Review summary from the second-pass quality reviewer (Markdown table). */
  review?: string;
  /** True when the server skipped the review pass for this draft (fallback). */
  degraded?: boolean;
}

export function ReviewCard({
  lyrics: initialLyrics,
  style: initialStyle,
  title: initialTitle,
  vocal: initialVocal,
  styleTags,
  onSubmit,
  creating,
  review,
  degraded,
}: ReviewCardProps) {
  const { t } = useI18n();
  const [lyrics, setLyrics] = useState(initialLyrics);
  const [style, setStyle] = useState(initialStyle);
  const [title, setTitle] = useState(initialTitle);
  const [vocal, setVocal] = useState(initialVocal);
  // Selection is tracked by the English tag (stable across language
  // switches); the label is display-only.
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [reviewExpanded, setReviewExpanded] = useState(false);

  const reviewFixes = review ? (review.match(/🔧/g) || []).length : 0;

  function toggleTag(tag: string) {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag],
    );
  }

  function handleSubmit() {
    // Providers take English style descriptors — never the localized label.
    const tagStyle = selectedTags.join(", ");
    onSubmit({
      lyrics: lyrics.trim(),
      style: tagStyle || style.trim() || "healing and warm",
      title: title.trim(),
      vocal,
    });
  }

  const disabled = creating;

  return (
    <div className="review-card" style={disabled ? { opacity: 0.55 } : undefined}>
      <div className="review-header">{t("review_title")}</div>
      <div className="review-sub">{t("review_sub")}</div>

      {review && (
        <div className={`review-badge ${reviewExpanded ? "expanded" : ""}`}>
          <button
            className="review-badge-btn"
            onClick={() => setReviewExpanded(!reviewExpanded)}
            aria-expanded={reviewExpanded}
          >
            <span className="review-badge-icon">&#10003;</span>
            {t("review_quality")}
            {reviewFixes > 0 && <span className="review-fixes">{t(reviewFixes === 1 ? "review_fixes_one" : "review_fixes_many", { n: reviewFixes })}</span>}
            <span className="review-chevron">{reviewExpanded ? "▲" : "▼"}</span>
          </button>
          {reviewExpanded && (
            <pre className="review-detail">{review}</pre>
          )}
        </div>
      )}
      {degraded && !review && (
        <div className="degraded-note">{t("review_skipped")}</div>
      )}
      <textarea
        className="review-textarea"
        value={lyrics}
        onChange={(e) => setLyrics(e.target.value)}
        disabled={disabled}
      />

      <div className="review-field">
        <div className="review-label">{t("free_style")}</div>
        <input
          type="text"
          className="review-input"
          value={style}
          onChange={(e) => setStyle(e.target.value)}
          placeholder={t("free_style")}
          disabled={disabled}
        />
        <div className="style-tags">
          {styleTags.filter((s) => s.label).map(({ label, tag }) => (
            <span
              key={tag}
              className={`style-tag ${selectedTags.includes(tag) ? "on" : ""}`}
              onClick={() => !disabled && toggleTag(tag)}
              role="button"
              tabIndex={0}
            >
              {label}
            </span>
          ))}
        </div>
      </div>

      <div className="review-field">
        <div className="review-label">{t("vocal")}</div>
        <div className="toggle-row">
          <button
            className={`toggle-btn ${vocal === "female" ? "on" : ""}`}
            onClick={() => !disabled && setVocal("female")}
            disabled={disabled}
          >
            {t("gender_female")}
          </button>
          <button
            className={`toggle-btn ${vocal === "male" ? "on" : ""}`}
            onClick={() => !disabled && setVocal("male")}
            disabled={disabled}
          >
            {t("gender_male")}
          </button>
        </div>
      </div>

      <div className="review-field">
        <div className="review-label">{t("free_title")}</div>
        <input
          type="text"
          className="review-input"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t("free_title_ph")}
          maxLength={50}
          disabled={disabled}
        />
      </div>

      <button
        className="btn-primary btn-lg btn-full"
        onClick={handleSubmit}
        disabled={disabled}
      >
        {creating ? (
          <>
            <span className="spinner" /> {t("creating")}
          </>
        ) : (
          t("create_song")
        )}
      </button>
    </div>
  );
}
