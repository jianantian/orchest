import { useRef, useState, type FormEvent } from "react";
import type { CreateGiftRequest } from "../types";
import { createGift, generateMusic, pollGenerateStatus } from "../api";
import { useI18n } from "../i18n";
import { MusicCard, type MusicCardState } from "./MusicCard";

const STYLE_CATALOG = [
  "pop", "rock", "rap", "electronic", "jazz", "classical", "folk", "r&b", "soul",
  "latin", "metal", "blues", "country", "punk",
  "romantic", "emotional", "melancholic", "upbeat", "energetic", "sentimental",
  "heartbreak", "reflective", "relaxing", "bittersweet", "nostalgic", "happy",
  "dark", "intense", "sad", "inspirational", "dramatic", "uplifting", "hopeful",
  "passionate", "peaceful", "dreamy", "fun", "moody", "positive", "empowering",
  "soothing", "haunting", "playful", "sorrow", "longing", "cheerful", "epic",
  "piano", "guitar", "strings", "synth", "orchestral", "drums", "bass", "violin",
  "saxophone", "flute", "cello", "acoustic", "electronic", "vocal",
  "danceable", "high energy", "mellow", "fast-paced", "slow", "soft", "powerful",
  "smooth", "rhythmic", "minimal", "atmospheric", "chill", "meditative",
  "healing", "warm", "light", "deep", "lively", "gentle", "bright", "moving",
];

function shuffle<T>(arr: T[]): T[] {
  const a = [...arr];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

export interface FreeCreatePanelProps {
  photos: string[];
  lang: string;
  onNavigate: (giftId: string) => void;
}

export function FreeCreatePanel({ photos, lang, onNavigate }: FreeCreatePanelProps) {
  const { t } = useI18n();
  const [lyrics, setLyrics] = useState("");
  const [selectedStyles, setSelectedStyles] = useState<string[]>([]);
  const [styleInput, setStyleInput] = useState("");
  const [vocal, setVocal] = useState("female");
  const [title, setTitle] = useState("");
  const [suggestions, setSuggestions] = useState<string[]>(() =>
    shuffle(STYLE_CATALOG).slice(0, 14),
  );
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [musicState, setMusicState] = useState<MusicCardState>("generating");
  const [giftId, setGiftId] = useState<string | null>(null);
  const [instrumental, setInstrumental] = useState(false);
  const lyricsRef = useRef<HTMLTextAreaElement>(null);

  function addStyle(style: string) {
    if (!style || selectedStyles.includes(style)) return;
    setSelectedStyles((prev) => [...prev, style]);
    refreshSuggestions([]);
  }

  function removeStyle(style: string) {
    setSelectedStyles((prev) => prev.filter((s) => s !== style));
  }

  function refreshSuggestions(exclude: string[]) {
    setSuggestions(
      shuffle(STYLE_CATALOG)
        .filter((s) => !exclude.includes(s) && !selectedStyles.includes(s))
        .slice(0, 14),
    );
  }

  function commitStyleInput() {
    const raw = styleInput.trim();
    if (!raw) return;
    raw
      .split(/[,，;；\n]/)
      .map((s) => s.trim())
      .filter(Boolean)
      .forEach(addStyle);
    setStyleInput("");
  }

  async function handleGenerate(e: FormEvent) {
    e.preventDefault();
    setCreating(true);
    setError(null);
    setMusicState("generating");

    try {
      const isInstrumental = instrumental && !lyrics.trim();
      const req: CreateGiftRequest = isInstrumental
        ? {
            kind: "instrumental",
            meta: { lang },
            photos,
            style: selectedStyles.join(", ") || "healing and warm",
            lyrics: "",
          }
        : {
            lyrics: lyrics.trim() || "instrumental",
            kind: "song",
            meta: {
              lang,
              title: title.trim(),
              vocal,
            },
            photos,
            style: selectedStyles.join(", ") || "healing and warm",
          };

      const res = await createGift(req);
      const id = res.id;
      setGiftId(id);

      await generateMusic(id);

      const poll = setInterval(async () => {
        try {
          const status = await pollGenerateStatus(id);
          if (status.status === "done") {
            clearInterval(poll);
            setMusicState("ready");
            setCreating(false);
          } else if (status.status === "failed") {
            clearInterval(poll);
            setMusicState("error");
            setCreating(false);
            setError("Generation failed");
          }
        } catch {
          // Keep polling
        }
      }, 3000);
    } catch (e) {
      setMusicState("error");
      setCreating(false);
      setError(e instanceof Error ? e.message : "Creation failed");
    }
  }

  function handleMusicOpen() {
    if (giftId) onNavigate(giftId);
  }

  async function handleRetry() {
    if (!giftId) return;
    setMusicState("generating");
    setError(null);
    try {
      await generateMusic(giftId);
      const poll = setInterval(async () => {
        try {
          const status = await pollGenerateStatus(giftId);
          if (status.status === "done") {
            clearInterval(poll);
            setMusicState("ready");
          } else if (status.status === "failed") {
            clearInterval(poll);
            setMusicState("error");
            setError("Generation failed");
          }
        } catch {
          // Keep polling
        }
      }, 3000);
    } catch (e) {
      setMusicState("error");
      setError(e instanceof Error ? e.message : "Retry failed");
    }
  }

  return (
    <div className="free-panel">
      {/* Lyrics card */}
      <div className="create-card">
        <div className="create-card-head">
          <span className="create-label">{t("free_lyrics")}</span>
          <label className="toggle-switch">
            <input
              type="checkbox"
              checked={instrumental}
              onChange={(e) => setInstrumental(e.target.checked)}
            />
            <span>{t("free_instrumental")}</span>
          </label>
        </div>
        <div className="lyrics-wrap">
          <textarea
            ref={lyricsRef}
            value={lyrics}
            onChange={(e) => setLyrics(e.target.value)}
            placeholder={t("paste_lyrics_ph")}
            disabled={instrumental}
            style={instrumental ? { opacity: 0.35 } : undefined}
          />
        </div>
        <div className="lyrics-actions-row" style={instrumental ? { opacity: 0.4, pointerEvents: "none" } : undefined}>
          <button className="btn-ghost" type="button">
            <span className="btn-icon">✨</span> {t("free_polish")}
          </button>
          <button className="btn-ghost" type="button">
            <span className="btn-icon">📝</span> {t("free_expand")}
          </button>
        </div>
      </div>

      {/* Style card */}
      <div className="create-card">
        <span className="create-label">{t("free_style")}</span>
        <div className="style-input-wrap">
          <textarea
            value={styleInput}
            onChange={(e) => setStyleInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commitStyleInput();
              } else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0) {
                removeStyle(selectedStyles[selectedStyles.length - 1]);
              }
            }}
            onBlur={commitStyleInput}
            placeholder={t("free_style_ph")}
            rows={2}
          />
        </div>
        {selectedStyles.length > 0 && (
          <div className="selected-styles">
            {selectedStyles.map((s) => (
              <span key={s} className="style-chip">
                {s}
                <span className="remove" onClick={() => removeStyle(s)} role="button" tabIndex={0}>
                  ×
                </span>
              </span>
            ))}
          </div>
        )}
        <div className="style-suggest-row">
          <button
            className="btn-refresh"
            type="button"
            onClick={() => refreshSuggestions(selectedStyles)}
          >
            🔄
          </button>
          <div className="style-suggest-strip">
            {suggestions.map((s) => (
              <button
                key={s}
                className="style-suggest-chip"
                type="button"
                onClick={() => addStyle(s)}
              >
                {s}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Vocal card */}
      {!instrumental && (
        <div className="create-card inline-row">
          <span className="create-label">{t("free_vocal")}</span>
          <div className="vocal-toggle">
            <button
              className={`vocal-btn ${vocal === "female" ? "on" : ""}`}
              onClick={() => setVocal("female")}
            >
              {t("review_vocal_female")}
            </button>
            <button
              className={`vocal-btn ${vocal === "male" ? "on" : ""}`}
              onClick={() => setVocal("male")}
            >
              {t("review_vocal_male")}
            </button>
          </div>
        </div>
      )}

      {/* Title card */}
      <div className="create-card">
        <span className="create-label">{t("free_title")}</span>
        <div className="title-input-wrap">
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder={t("free_title_ph")}
            maxLength={50}
          />
          <span className="title-char-count">{title.length}/50</span>
        </div>
      </div>

      <button
        className="btn-primary"
        onClick={handleGenerate}
        disabled={creating}
      >
        {creating ? (
          <>
            <span className="spinner" /> {t("free_generating")}
          </>
        ) : (
          t("free_generate")
        )}
      </button>

      {error && <div className="error-msg">{error}</div>}

      {giftId && (
        <MusicCard
          initialState={musicState}
          onOpen={handleMusicOpen}
          onRetry={handleRetry}
        />
      )}
    </div>
  );
}
