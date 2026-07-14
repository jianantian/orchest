import { useState } from "react";

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
  styleTags: string[];
  onSubmit: (data: ReviewData) => void;
  creating: boolean;
}

export function ReviewCard({
  lyrics: initialLyrics,
  style: initialStyle,
  title: initialTitle,
  vocal: initialVocal,
  styleTags,
  onSubmit,
  creating,
}: ReviewCardProps) {
  const [lyrics, setLyrics] = useState(initialLyrics);
  const [style, setStyle] = useState(initialStyle);
  const [title, setTitle] = useState(initialTitle);
  const [vocal, setVocal] = useState(initialVocal);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);

  function toggleTag(tag: string) {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag],
    );
  }

  function handleSubmit() {
    const tagStyle = selectedTags.join("、");
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
      <div className="review-header">Review your song</div>
      <div className="review-sub">Edit anything before creating</div>

      <textarea
        className="review-textarea"
        value={lyrics}
        onChange={(e) => setLyrics(e.target.value)}
        disabled={disabled}
      />

      <div className="review-field">
        <div className="review-label">Style</div>
        <input
          type="text"
          className="review-input"
          value={style}
          onChange={(e) => setStyle(e.target.value)}
          placeholder="Style"
          disabled={disabled}
        />
        <div className="style-tags">
          {styleTags.filter(Boolean).map((tag) => (
            <span
              key={tag}
              className={`style-tag ${selectedTags.includes(tag) ? "on" : ""}`}
              onClick={() => !disabled && toggleTag(tag)}
              role="button"
              tabIndex={0}
            >
              {tag}
            </span>
          ))}
        </div>
      </div>

      <div className="review-field">
        <div className="review-label">Vocal</div>
        <div className="toggle-row">
          <button
            className={`toggle-btn ${vocal === "female" ? "on" : ""}`}
            onClick={() => !disabled && setVocal("female")}
            disabled={disabled}
          >
            Female
          </button>
          <button
            className={`toggle-btn ${vocal === "male" ? "on" : ""}`}
            onClick={() => !disabled && setVocal("male")}
            disabled={disabled}
          >
            Male
          </button>
        </div>
      </div>

      <div className="review-field">
        <div className="review-label">Title</div>
        <input
          type="text"
          className="review-input"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Song title"
          maxLength={50}
          disabled={disabled}
        />
      </div>

      <button
        className="btn-primary btn-full"
        onClick={handleSubmit}
        disabled={disabled}
      >
        {creating ? (
          <>
            <span className="spinner" /> Creating…
          </>
        ) : (
          "Create Song"
        )}
      </button>
    </div>
  );
}
