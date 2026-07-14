import { useRef, useState, type FormEvent } from "react";
import type { ChatMessage } from "../types";
import { streamChat } from "../api";
import { useI18n } from "../i18n";
import { shuffleStyles } from "../lib/styles";
import { useMusicGen } from "../hooks/useMusicGen";
import { MusicCard } from "./MusicCard";

export interface FreeCreatePanelProps {
  photos: string[];
  lang: string;
  onNavigate: (giftId: string) => void;
}

export function FreeCreatePanel({ photos, lang, onNavigate }: FreeCreatePanelProps) {
  const { t } = useI18n();
  const gen = useMusicGen();

  const [lyrics, setLyrics] = useState("");
  const [selectedStyles, setSelectedStyles] = useState<string[]>([]);
  const [styleInput, setStyleInput] = useState("");
  const [vocal, setVocal] = useState("female");
  const [title, setTitle] = useState("");
  const [suggestions, setSuggestions] = useState<string[]>(() => shuffleStyles([], 14));
  const [instrumental, setInstrumental] = useState(false);
  const [polishing, setPolishing] = useState(false);
  const [expanding, setExpanding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lyricsRef = useRef<HTMLTextAreaElement>(null);

  function addStyle(style: string) {
    if (!style || selectedStyles.includes(style)) return;
    setSelectedStyles((prev) => [...prev, style]);
    setSuggestions(shuffleStyles([...selectedStyles, style], 14));
  }

  function removeStyle(style: string) {
    setSelectedStyles((prev) => prev.filter((s) => s !== style));
  }

  function refreshSuggestions() {
    setSuggestions(shuffleStyles(selectedStyles, 14));
  }

  function commitStyleInput() {
    const raw = styleInput.trim();
    if (!raw) return;
    raw.split(/[,，;；\n]/).map((s) => s.trim()).filter(Boolean).forEach(addStyle);
    setStyleInput("");
  }

  async function callLyricsAPI(action: "polish" | "expand", input: string): Promise<string> {
    const systemMsg = action === "polish"
      ? "You are a professional lyric editor. Polish the following lyrics, preserving meaning and emotion while improving expression and rhythm. Output ONLY the polished lyrics. No explanations, no markup."
      : "You are a professional songwriter. Expand the user's input into complete song lyrics with structural tags like [verse], [chorus], [bridge]. Output ONLY the lyrics. No explanations.";
    const userMsg = action === "polish" ? `Polish these lyrics:\n\n${input}` : `Expand into a complete song:\n\n${input}`;
    const messages: ChatMessage[] = [
      { role: "system", content: systemMsg },
      { role: "user", content: userMsg },
    ];
    let full = "";
    for await (const event of streamChat({ messages, meta: { lang }, photos: [] })) {
      if (event.type === "Delta") full += event.text;
    }
    return full.trim() || input;
  }

  async function handlePolish() {
    const text = lyrics.trim();
    if (!text) return;
    setPolishing(true);
    try { setLyrics(await callLyricsAPI("polish", text)); } catch { /* best-effort */ }
    finally { setPolishing(false); }
  }

  async function handleExpand() {
    const text = lyrics.trim();
    if (!text) return;
    setExpanding(true);
    try { setLyrics(await callLyricsAPI("expand", text)); } catch { /* best-effort */ }
    finally { setExpanding(false); }
  }

  async function handleGenerate(e: FormEvent) {
    e.preventDefault();
    setError(null);

    if (instrumental && !lyrics.trim()) {
      await gen.start({
        lyrics: "",
        style: selectedStyles.join(", ") || "healing and warm",
        lang,
        photos,
      });
    } else {
      await gen.start({
        lyrics: lyrics.trim() || "instrumental",
        style: selectedStyles.join(", ") || "healing and warm",
        title: title.trim(),
        vocal,
        lang,
        photos,
      });
    }
    if (gen.error) setError(gen.error);
  }

  const musicState = gen.state === "idle" ? "generating" as const
    : gen.state === "ready" ? "ready" as const
    : gen.state === "error" ? "error" as const
    : "generating" as const;

  return (
    <div className="free-panel">
      {/* Lyrics card */}
      <div className="create-card">
        <div className="create-card-head">
          <span className="create-label">{t("free_lyrics")}</span>
          <label className="toggle-switch">
            <input type="checkbox" checked={instrumental} onChange={(e) => setInstrumental(e.target.checked)} />
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
        {!instrumental && (
          <div className="lyrics-actions-row">
            <button className={`btn-ghost${polishing ? " loading" : ""}`} type="button" onClick={handlePolish} disabled={polishing}>
              {polishing ? <span className="spinner" /> : <span className="btn-icon">✨</span>}
              {polishing ? "" : t("free_polish")}
            </button>
            <button className={`btn-ghost${expanding ? " loading" : ""}`} type="button" onClick={handleExpand} disabled={expanding}>
              {expanding ? <span className="spinner" /> : <span className="btn-icon">📝</span>}
              {expanding ? "" : t("free_expand")}
            </button>
          </div>
        )}
      </div>

      {/* Style card */}
      <div className="create-card">
        <span className="create-label">{t("free_style")}</span>
        <div className="style-input-wrap">
          <textarea
            value={styleInput}
            onChange={(e) => setStyleInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); commitStyleInput(); }
              else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0) {
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
                <span className="remove" onClick={() => removeStyle(s)} role="button" tabIndex={0}>×</span>
              </span>
            ))}
          </div>
        )}
        <div className="style-suggest-row">
          <button className="btn-refresh" type="button" onClick={refreshSuggestions}>🔄</button>
          <div className="style-suggest-strip">
            {suggestions.map((s) => (
              <button key={s} className="style-suggest-chip" type="button" onClick={() => addStyle(s)}>{s}</button>
            ))}
          </div>
        </div>
      </div>

      {/* Vocal card */}
      {!instrumental && (
        <div className="create-card inline-row">
          <span className="create-label">{t("free_vocal")}</span>
          <div className="vocal-toggle">
            <button className={`vocal-btn ${vocal === "female" ? "on" : ""}`} onClick={() => setVocal("female")}>{t("review_vocal_female")}</button>
            <button className={`vocal-btn ${vocal === "male" ? "on" : ""}`} onClick={() => setVocal("male")}>{t("review_vocal_male")}</button>
          </div>
        </div>
      )}

      {/* Title card */}
      <div className="create-card">
        <span className="create-label">{t("free_title")}</span>
        <div className="title-input-wrap">
          <input type="text" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={t("free_title_ph")} maxLength={50} />
          <span className="title-char-count">{title.length}/50</span>
        </div>
      </div>

      <button className="btn-primary btn-lg btn-full" onClick={handleGenerate} disabled={gen.state === "generating"}>
        {gen.state === "generating" ? <><span className="spinner" /> {t("free_generating")}</> : t("free_generate")}
      </button>

      {(error || gen.error) && <div className="error-msg">{error || gen.error}</div>}

      {gen.giftId && (
        <MusicCard initialState={musicState} onOpen={() => onNavigate(gen.giftId!)} onRetry={() => gen.retry(gen.giftId!)} />
      )}
    </div>
  );
}
