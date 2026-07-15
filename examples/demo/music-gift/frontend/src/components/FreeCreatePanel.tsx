import { useRef, useState, type FormEvent } from "react";
import type { ChatMessage } from "../types";
import { streamChat } from "../api";
import { useI18n } from "../i18n";
import { shuffleStyles } from "../lib/styles";
import { useMusicGen } from "../hooks/useMusicGen";
import { MusicCard } from "./MusicCard";

export interface FreeCreatePanelProps { photos: string[]; lang: string; onNavigate: (giftId: string) => void; }

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

  function addStyle(s: string) { if (s && !selectedStyles.includes(s)) setSelectedStyles(p => [...p, s]); }
  function removeStyle(s: string) { setSelectedStyles(p => p.filter(x => x !== s)); }
  function refreshSuggestions() { setSuggestions(shuffleStyles(selectedStyles, 14)); }
  function commitStyleInput() { const t = styleInput.trim(); if (t) addStyle(t); setStyleInput(""); }

  async function handlePolish() {
    const styleStr = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    if (!styleStr) return;
    setPolishing(true);
    try {
      const res = await fetch("/api/polish-music-prompt", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ lyrics: lyrics.trim() || undefined, style: styleStr, vocal, provider: "suno" }) });
      if (res.ok) { const d = await res.json(); setStyleInput(d.prompt); }
    } catch { /* best-effort */ }
    finally { setPolishing(false); }
  }

  async function handleExpand() {
    if (!lyrics.trim()) return;
    setExpanding(true);
    try {
      const msgs: ChatMessage[] = [{ role: "system", content: "Expand into complete lyrics with [verse],[chorus],[bridge] tags. Output ONLY the lyrics." }, { role: "user", content: `Expand:\n\n${lyrics.trim()}` }];
      let full = "";
      for await (const e of streamChat({ messages: msgs, meta: { lang }, photos: [] })) { if (e.type === "Delta") full += e.text; }
      const r = full.trim(); if (r) setLyrics(r);
    } catch { /* best-effort */ }
    finally { setExpanding(false); }
  }

  async function handleGenerate(e: FormEvent) {
    e.preventDefault(); setError(null);
    const style = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    await gen.start({ lyrics: instrumental ? "" : lyrics.trim() || "instrumental", style, vocal: instrumental ? undefined : vocal, lang, photos });
    if (gen.error) setError(gen.error);
  }

  const musicState = gen.state === "idle" ? "generating" as const : gen.state === "ready" ? "ready" as const : gen.state === "error" ? "error" as const : "generating" as const;

  return (
    <div className="free-panel">
      {/* ① Lyrics */}
      <div className="create-card">
        <div className="create-card-head">
          <span className="create-label">{t("free_lyrics")}</span>
          {!instrumental && (
            <button className={`btn-ghost btn-sm${expanding ? " loading" : ""}`} type="button" onClick={handleExpand} disabled={expanding}>
              {expanding ? <span className="spinner" /> : "📝"} {t("free_expand")}
            </button>
          )}
        </div>
        <div className="lyrics-wrap">
          <textarea ref={lyricsRef} value={lyrics} onChange={e => setLyrics(e.target.value)}
            placeholder={t("paste_lyrics_ph")} disabled={instrumental}
            style={instrumental ? { opacity: 0.35 } : undefined} />
        </div>
      </div>

      {/* ② Style + Vocal + Instrumental */}
      <div className="create-card">
        <span className="create-label">{t("free_style")}</span>
        <div className="style-input-wrap">
          <textarea value={styleInput} onChange={e => setStyleInput(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter") { e.preventDefault(); commitStyleInput(); } else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0) removeStyle(selectedStyles[selectedStyles.length - 1]); }}
            onBlur={commitStyleInput} placeholder={t("free_style_ph")} rows={2} />
          <button className="btn-ghost btn-sm" type="button" onClick={handlePolish} disabled={polishing}>
            {polishing ? <span className="spinner" /> : "✨"} Polish
          </button>
        </div>
        {selectedStyles.length > 0 && (
          <div className="selected-styles">{selectedStyles.map(s => <span key={s} className="style-chip">{s}<span className="remove" onClick={() => removeStyle(s)} role="button" tabIndex={0}>×</span></span>)}</div>
        )}
        <div className="vocal-row-unified">
          <button className={`vocal-chip ${!instrumental && vocal === "female" ? "on" : ""}`} onClick={() => { setInstrumental(false); setVocal("female"); }}>♀ Female</button>
          <button className={`vocal-chip ${!instrumental && vocal === "male" ? "on" : ""}`} onClick={() => { setInstrumental(false); setVocal("male"); }}>♂ Male</button>
          <button className={`vocal-chip ${instrumental ? "on" : ""}`} onClick={() => { setInstrumental(true); }}>🎵 Instrumental</button>
        </div>
        <div className="style-suggest-row">
          <button className="btn-refresh" type="button" onClick={refreshSuggestions}>🔄</button>
          <div className="style-suggest-strip">{suggestions.map(s => <button key={s} className="style-suggest-chip" type="button" onClick={() => addStyle(s)}>{s}</button>)}</div>
        </div>
      </div>

      {/* ③ Title */}
      <div className="create-card">
        <span className="create-label">{t("free_title")}</span>
        <div className="title-input-wrap">
          <input type="text" value={title} onChange={e => setTitle(e.target.value)} placeholder={t("free_title_ph")} maxLength={50} />
          <span className="title-char-count">{title.length}/50</span>
        </div>
      </div>

      <button className="btn-primary btn-lg btn-full" onClick={handleGenerate} disabled={gen.state === "generating"}>
        {gen.state === "generating" ? <><span className="spinner" /> {t("free_generating")}</> : "🎵 " + (t("free_generate") || "Create Song")}
      </button>

      {(error || gen.error) && <div className="error-msg">{error || gen.error}</div>}

      {gen.giftId && <MusicCard initialState={musicState} onOpen={() => onNavigate(gen.giftId!)} onRetry={() => gen.retry(gen.giftId!)} />}
    </div>
  );
}
