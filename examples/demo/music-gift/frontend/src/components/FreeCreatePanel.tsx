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
  const [lyricsMode, setLyricsMode] = useState<"auto" | "write">("auto");
  const [styleInput, setStyleInput] = useState("");
  const [selectedStyles, setSelectedStyles] = useState<string[]>([]);
  const [suggestions, setSuggestions] = useState<string[]>(() => shuffleStyles([], 14));
  const [vocal, setVocal] = useState("female");
  const [instrumental, setInstrumental] = useState(false);
  const [polishing, setPolishing] = useState(false);
  const [expanding, setExpanding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lyricsRef = useRef<HTMLTextAreaElement>(null);

  function addStyle(s: string) {
    if (!s || selectedStyles.includes(s)) return;
    setSelectedStyles((prev) => [...prev, s]);
  }
  function removeStyle(s: string) {
    setSelectedStyles((prev) => prev.filter((x) => x !== s));
  }
  function commitStyleInput() {
    const text = styleInput.trim();
    if (text) addStyle(text);
    setStyleInput("");
  }
  function refreshSuggestions() {
    setSuggestions(shuffleStyles(selectedStyles, 14));
  }

  async function handlePolish() {
    const styleStr = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    if (!styleStr) return;
    setPolishing(true);
    try {
      const res = await fetch("/api/polish-music-prompt", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          lyrics: lyrics.trim() || undefined,
          style: styleStr,
          vocal,
          provider: "suno",
        }),
      });
      if (res.ok) {
        const data = await res.json();
        setStyleInput(data.prompt);
      }
    } catch { /* best-effort */ }
    finally { setPolishing(false); }
  }

  async function handleExpand() {
    if (!lyrics.trim()) return;
    setExpanding(true);
    try {
      const messages: ChatMessage[] = [
        { role: "system", content: "Expand into complete lyrics with [verse], [chorus], [bridge] tags. Output ONLY the lyrics." },
        { role: "user", content: `Expand:\n\n${lyrics.trim()}` },
      ];
      let full = "";
      for await (const event of streamChat({ messages, meta: { lang }, photos: [] })) {
        if (event.type === "Delta") full += event.text;
      }
      const result = full.trim();
      if (result) setLyrics(result);
    } catch { /* best-effort */ }
    finally { setExpanding(false); }
  }

  async function handleGenerate(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const style = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    if (instrumental && !lyrics.trim()) {
      await gen.start({ lyrics: "", style, lang, photos });
    } else {
      await gen.start({ lyrics: lyrics.trim() || "instrumental", style, vocal, lang, photos });
    }
    if (gen.error) setError(gen.error);
  }

  const musicState = gen.state === "idle" ? "generating" as const
    : gen.state === "ready" ? "ready" as const
    : gen.state === "error" ? "error" as const
    : "generating" as const;

  return (
    <div className="free-panel suno-advanced">
      {/* === 1. Describe your lyrics === */}
      <div className="create-card">
        <div className="create-card-head">
          <span className="create-label">{t("free_lyrics_desc") || "Describe your lyrics"}</span>
          <div className="tab-group">
            <button className={`tab-btn ${lyricsMode === "auto" ? "active" : ""}`} onClick={() => setLyricsMode("auto")}>Auto</button>
            <button className={`tab-btn ${lyricsMode === "write" ? "active" : ""}`} onClick={() => setLyricsMode("write")}>Write Lyrics</button>
          </div>
        </div>
        <div className="lyrics-wrap">
          <textarea
            ref={lyricsRef}
            value={lyrics}
            onChange={(e) => setLyrics(e.target.value)}
            placeholder={lyricsMode === "auto" ? "Describe what the song should be about…" : "Write your own lyrics with [verse], [chorus], [bridge] tags…"}
            disabled={instrumental}
            style={instrumental ? { opacity: 0.35 } : undefined}
            rows={lyricsMode === "write" ? 5 : 3}
          />
        </div>
        <div className="card-foot-row">
          <label className="toggle-switch">
            <input type="checkbox" checked={instrumental} onChange={(e) => setInstrumental(e.target.checked)} />
            <span>{t("free_instrumental")}</span>
          </label>
          <div className="vocal-toggle inline">
            <button className={`vocal-btn ${vocal === "female" ? "on" : ""}`} onClick={() => setVocal("female")}>♀</button>
            <button className={`vocal-btn ${vocal === "male" ? "on" : ""}`} onClick={() => setVocal("male")}>♂</button>
          </div>
          {lyricsMode === "write" && (
            <button className={`btn-ghost btn-sm${expanding ? " loading" : ""}`} type="button" onClick={handleExpand} disabled={expanding}>
              {expanding ? <span className="spinner" /> : "📝"} Expand
            </button>
          )}
        </div>
      </div>

      {/* === 2. Styles === */}
      <div className="create-card">
        <div className="create-card-head">
          <span className="create-label">Styles</span>
          <button className={`btn-ghost btn-sm${polishing ? " loading" : ""}`} type="button" onClick={handlePolish} disabled={polishing}>
            {polishing ? <span className="spinner" /> : "✨"} Polish
          </button>
        </div>
        <div className="style-input-wrap">
          <input
            type="text"
            value={styleInput}
            onChange={(e) => setStyleInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); commitStyleInput(); }
              else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0)
                removeStyle(selectedStyles[selectedStyles.length - 1]);
            }}
            placeholder="Enter style tags"
          />
        </div>
        {selectedStyles.length > 0 && (
          <div className="selected-styles">
            {selectedStyles.map((s) => (
              <span key={s} className="style-chip">{s}<span className="remove" onClick={() => removeStyle(s)} role="button" tabIndex={0}>×</span></span>
            ))}
          </div>
        )}
        <div className="style-preset-grid">
          {suggestions.map((s) => (
            <button key={s} className="style-preset-btn" type="button" onClick={() => addStyle(s)}>{s}</button>
          ))}
          <button className="style-preset-btn refresh-btn" type="button" onClick={refreshSuggestions}>🔄</button>
        </div>
      </div>

      {/* === 3. Create === */}
      <button className="btn-primary btn-lg btn-full" onClick={handleGenerate} disabled={gen.state === "generating"}>
        {gen.state === "generating" ? <><span className="spinner" /> {t("free_generating")}</> : "🎵 Create"}
      </button>

      {(error || gen.error) && <div className="error-msg">{error || gen.error}</div>}

      {gen.giftId && (
        <MusicCard initialState={musicState} onOpen={() => onNavigate(gen.giftId!)} onRetry={() => gen.retry(gen.giftId!)} />
      )}
    </div>
  );
}
