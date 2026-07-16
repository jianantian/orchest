import { useRef, useState, type FormEvent } from "react";
import { streamChat } from "../api";
import { useI18n } from "../i18n";
import { shuffleStyles } from "../lib/styles";
import { useMusicGen } from "../hooks/useMusicGen";
import { MusicCard } from "./MusicCard";
import { MicIcon, MusicNoteIcon, PencilIcon, SparklesIcon, ExpandIcon, XIcon, FemaleIcon, MaleIcon } from "./Icons";

export interface FreeCreatePanelProps { photos: string[]; lang: string; onNavigate: (giftId: string) => void; }

export function FreeCreatePanel({ photos, lang, onNavigate }: FreeCreatePanelProps) {
  const { t } = useI18n();
  const gen = useMusicGen();

  const [instrumental, setInstrumental] = useState(false);
  const [vocalGender, setVocalGender] = useState<"female" | "male" | null>(null);
  const [showMoreOptions, setShowMoreOptions] = useState(false);
  const [lyrics, setLyrics] = useState("");
  const [selectedStyles, setSelectedStyles] = useState<string[]>([]);
  const [styleInput, setStyleInput] = useState("");
  const [title, setTitle] = useState("");
  const [suggestions, setSuggestions] = useState<string[]>(() => shuffleStyles([], 14));
  const [polishing, setPolishing] = useState(false);
  const [polishPrompt, setPolishPrompt] = useState("");
  const [showPolishPrompt, setShowPolishPrompt] = useState(false);
  const [writing, setWriting] = useState(false);
  const [editing, setEditing] = useState(false);
  const [expanding, setExpanding] = useState(false);
  const [promptInput, setPromptInput] = useState("");
  const [showPromptBar, setShowPromptBar] = useState<"write" | "edit" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const lyricsRef = useRef<HTMLTextAreaElement>(null);

  const vocal = !instrumental ? vocalGender || undefined : undefined;

  function addStyle(s: string) { if (s && !selectedStyles.includes(s)) setSelectedStyles(p => [...p, s]); }
  function removeStyle(s: string) { setSelectedStyles(p => p.filter(x => x !== s)); }
  function refreshSuggestions() { setSuggestions(shuffleStyles(selectedStyles, 14)); }
  function commitStyleInput() { const t = styleInput.trim(); if (t) addStyle(t); setStyleInput(""); }

  async function handlePolish() {
    setShowPolishPrompt(false);
    const base = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    if (!base) return;
    setPolishing(true);
    try {
      const res = await fetch("/api/polish-music-prompt", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ lyrics: lyrics.trim() || undefined, style: base, prompt: polishPrompt.trim() || undefined, vocal, provider: "suno" }) });
      if (res.ok) { const d = await res.json(); setStyleInput(d.prompt); setPolishPrompt(""); }
    } catch { /* */ }
    finally { setPolishing(false); }
  }

  async function handleAiAction(action: "write" | "edit") {
    const prompt = promptInput.trim();
    if (!prompt) return;
    setShowPromptBar(null); setPromptInput("");
    if (action === "write") setWriting(true); else setEditing(true);
    try {
      const s = action === "write"
        ? "You are a professional songwriter. Write complete song lyrics with [verse], [chorus], [bridge] tags based on the user's description. Output ONLY the lyrics."
        : "You are a professional lyric editor. Edit the provided lyrics based on the user's instructions. Keep structure. Output ONLY the edited lyrics.";
      const u = action === "write" ? prompt : `Instructions: ${prompt}\n\nOriginal lyrics:\n${lyrics}`;
      let full = "";
      for await (const e of streamChat({ messages: [{ role: "system", content: s }, { role: "user", content: u }], meta: { lang }, photos: [] })) { if (e.type === "Delta") full += e.text; }
      if (full.trim()) setLyrics(full.trim());
    } catch { /* */ }
    finally { if (action === "write") setWriting(false); else setEditing(false); }
  }

  async function handleExpand() {
    if (!lyrics.trim()) return;
    setExpanding(true);
    try {
      let full = "";
      for await (const e of streamChat({ messages: [{ role: "system", content: "Expand into complete lyrics with [verse],[chorus],[bridge] tags. Output ONLY the lyrics." }, { role: "user", content: `Expand:\n\n${lyrics.trim()}` }], meta: { lang }, photos: [] })) { if (e.type === "Delta") full += e.text; }
      if (full.trim()) setLyrics(full.trim());
    } catch { /* */ }
    finally { setExpanding(false); }
  }

  async function handleGenerate(e: FormEvent) {
    e.preventDefault(); setError(null);
    const style = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    await gen.start({ lyrics: instrumental ? "" : lyrics.trim() || "instrumental", style, vocal, lang, photos });
    if (gen.error) setError(gen.error);
  }

  const musicState = gen.state === "idle" ? "generating" as const : gen.state === "ready" ? "ready" as const : gen.state === "error" ? "error" as const : "generating" as const;

  return (
    <div className="free-panel editorial">
      {/* ═══ Top: Vocal / Instrumental ═══ */}
      <div className="mode-bar">
        <button className={`mode-btn ${!instrumental ? "active" : ""}`} onClick={() => setInstrumental(false)}>
          <span className="mode-icon"><MicIcon /></span>
          <span className="mode-label">Vocal</span>
        </button>
        <button className={`mode-btn ${instrumental ? "active" : ""}`} onClick={() => setInstrumental(true)}>
          <span className="mode-icon"><MusicNoteIcon /></span>
          <span className="mode-label">Instrumental</span>
        </button>
      </div>

      {/* ═══ Lyrics ═══ */}
      <section className="editorial-section">
        <div className="section-header">
          <h3 className="section-title">{t("free_lyrics")}</h3>
          <div className="section-actions">
            {!instrumental && <>
              <button className={`btn-ghost btn-sm${writing ? " loading" : ""}`} onClick={() => setShowPromptBar(showPromptBar === "write" ? null : "write")} disabled={writing}>{writing ? <span className="spinner" /> : <PencilIcon />} Write</button>
              {lyrics.trim() && <button className={`btn-ghost btn-sm${editing ? " loading" : ""}`} onClick={() => setShowPromptBar(showPromptBar === "edit" ? null : "edit")} disabled={editing}>{editing ? <span className="spinner" /> : <SparklesIcon />} Edit</button>}
              <button className={`btn-ghost btn-sm${expanding ? " loading" : ""}`} onClick={handleExpand} disabled={expanding}>{expanding ? <span className="spinner" /> : <ExpandIcon />} Expand</button>
            </>}
          </div>
        </div>
        {showPromptBar && (
          <div className="prompt-bar">
            <input type="text" className="prompt-bar-input" value={promptInput} onChange={e => setPromptInput(e.target.value)}
              onKeyDown={e => { if (e.key === "Enter") handleAiAction(showPromptBar); else if (e.key === "Escape") { setShowPromptBar(null); setPromptInput(""); } }}
              placeholder={showPromptBar === "write" ? "Describe the song you want…" : "How should I edit the lyrics?"}
              autoFocus />
          </div>
        )}
        <textarea ref={lyricsRef} className="lyrics-manuscript" value={lyrics} onChange={e => setLyrics(e.target.value)}
          placeholder={instrumental ? "Instrumental, no lyrics needed" : t("paste_lyrics_ph")}
          disabled={instrumental} rows={instrumental ? 2 : 7} />
      </section>

      {/* ═══ Style ═══ */}
      <section className="editorial-section">
        <div className="section-header">
          <h3 className="section-title">{t("free_style")}</h3>
          <button className="btn-ghost btn-sm" onClick={() => setShowPolishPrompt(!showPolishPrompt)}>{showPolishPrompt ? <XIcon /> : <SparklesIcon />} Personalize</button>
        </div>
        {showPolishPrompt && (
          <div className="prompt-bar">
            <input type="text" className="prompt-bar-input" value={polishPrompt} onChange={e => setPolishPrompt(e.target.value)}
              onKeyDown={e => { if (e.key === "Enter") handlePolish(); else if (e.key === "Escape") { setShowPolishPrompt(false); setPolishPrompt(""); } }}
              placeholder="Describe the vibe you want, e.g. 'warmer, more romantic, add strings'"
              autoFocus />
          </div>
        )}
        {polishing && <p className="polish-status"><span className="spinner" /> Personalizing…</p>}
        <div className="style-composer">
          <input type="text" className="style-input" value={styleInput} onChange={e => setStyleInput(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter") { e.preventDefault(); commitStyleInput(); } else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0) removeStyle(selectedStyles[selectedStyles.length - 1]); }}
            placeholder={t("free_style_ph")} />
          {selectedStyles.length > 0 && <div className="style-chips">{selectedStyles.map(s => <span key={s} className="style-chip" onClick={() => removeStyle(s)} role="button" tabIndex={0} onKeyDown={e => e.key === "Enter" && removeStyle(s)}>{s} <XIcon /></span>)}</div>}
          <div className="style-suggestions">
            <button className="suggest-refresh" onClick={refreshSuggestions} title="More styles" aria-label="Refresh style suggestions">↻</button>
            {suggestions.map(s => <button key={s} className="suggest-chip" onClick={() => addStyle(s)}>{s}</button>)}
          </div>

          {/* Vocal Gender — in More Options */}
          {!instrumental && (
            <div className="more-options" data-open={showMoreOptions ? "true" : "false"}>
              <button className="btn-more" type="button" onClick={() => setShowMoreOptions(!showMoreOptions)}>
                <span className="t-acc-chevron">
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M4 6.5L8 10.5L12 6.5"/></svg>
                </span>
                More Options
              </button>
              <div className="t-acc-panel">
                <div className="more-body t-acc-panel-inner">
                  <p className="more-label">Vocal Gender</p>
                  <div className="gender-select">
                    <button className={`gender-opt ${vocalGender === "female" ? "on" : ""}`} onClick={() => setVocalGender(vocalGender === "female" ? null : "female")}><FemaleIcon /> Female</button>
                    <button className={`gender-opt ${vocalGender === "male" ? "on" : ""}`} onClick={() => setVocalGender(vocalGender === "male" ? null : "male")}><MaleIcon /> Male</button>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </section>

      {/* ═══ Title ═══ */}
      <section className="editorial-section">
        <h3 className="section-title">{t("free_title")}</h3>
        <input type="text" className="title-input" value={title} onChange={e => setTitle(e.target.value)} placeholder={t("free_title_ph")} maxLength={50} />
      </section>

      <button className="btn-create" onClick={handleGenerate} disabled={gen.state === "generating"}>
        {gen.state === "generating" ? <><span className="spinner" /> Generating…</> : "Create Song"}
      </button>

      {(error || gen.error) && <p className="error-msg" role="alert">{error || gen.error}</p>}
      {gen.giftId && <MusicCard initialState={musicState} onOpen={() => onNavigate(gen.giftId!)} onRetry={() => gen.retry(gen.giftId!)} />}
    </div>
  );
}
