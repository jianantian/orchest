import { useEffect, useRef, useState, type FormEvent } from "react";
import { streamChat, getGift, updateGift, regenerateGift, getGiftVersions, watchGeneration, type GenerationWatch } from "../api";
import { useI18n } from "../i18n";
import { shuffleStyles, stripMarkers } from "../lib/styles";
import { isImeComposing } from "../lib/ime";
import { creatorToken } from "../lib/creator";
import { useAuth } from "../hooks/useAuth";
import { useMusicGen } from "../hooks/useMusicGen";
import { MusicCard } from "./MusicCard";
import AudioPlayer from "./AudioPlayer";
import { LRCViewer } from "./LRCViewer";
import { parseLRC } from "../lib/lrc";
import { MicIcon, MusicNoteIcon, XIcon, FemaleIcon, MaleIcon } from "./Icons";
import type { ChatMessage, Gift, GiftVersion, SseEvent } from "../types";

export interface StudioProps { photos: string[]; lang: string; onNavigate: (giftId: string) => void; editGiftId?: string }

/** Draft fields the AI can change in a studio turn. */
type StudioField = "lyrics" | "style" | "title" | "vocal";

/** The whole draft, captured before an AI-applied change so it can be undone. */
interface DraftSnapshot {
  lyrics: string;
  selectedStyles: string[];
  styleInput: string;
  vocalGender: "female" | "male" | null;
  instrumental: boolean;
  title: string;
}

/** A chat turn; `note` is the "Updated: …" confirmation attached to the
 * assistant bubble after its changes were applied to the draft. */
interface StudioMessage extends ChatMessage { note?: string }

type DoneEvent = Extract<SseEvent, { type: "Done" }>;

const DRAFT_KEY = "moment_studio_draft";
const UNDO_CAP = 20;
const FLASH_MS = 1600;

interface SavedDraft extends DraftSnapshot { lang: string }

function loadDraft(lang: string): DraftSnapshot | null {
  try {
    const v = sessionStorage.getItem(DRAFT_KEY);
    if (!v) return null;
    const r = JSON.parse(v) as Partial<SavedDraft>;
    if (r.lang !== lang) return null;
    return {
      lyrics: typeof r.lyrics === "string" ? r.lyrics : "",
      selectedStyles: Array.isArray(r.selectedStyles) ? r.selectedStyles.filter(s => typeof s === "string") : [],
      styleInput: typeof r.styleInput === "string" ? r.styleInput : "",
      vocalGender: r.vocalGender === "female" || r.vocalGender === "male" ? r.vocalGender : null,
      instrumental: r.instrumental === true,
      title: typeof r.title === "string" ? r.title : "",
    };
  } catch { return null; }
}

/**
 * Hand a guided-flow draft to the studio through the same sessionStorage
 * channel the studio restores from on mount (the shape above is the wire
 * format — keep them in sync). Called right before switching to the studio
 * tab, so the next mount picks it up.
 */
export function stageStudioDraft(lang: string, fields: { lyrics: string; style: string; title: string; vocal: string }) {
  const d: SavedDraft = {
    lang,
    lyrics: fields.lyrics,
    selectedStyles: [],
    styleInput: fields.style,
    vocalGender: fields.vocal === "male" ? "male" : fields.vocal === "female" ? "female" : null,
    instrumental: false,
    title: fields.title,
  };
  try { sessionStorage.setItem(DRAFT_KEY, JSON.stringify(d)); } catch { /* quota */ }
}

export function Studio({ photos, lang, onNavigate, editGiftId }: StudioProps) {
  const { t } = useI18n();
  const { user } = useAuth();
  const gen = useMusicGen();

  // Read sessionStorage once per mount, not on every render. Edit mode fills
  // the draft from the gift instead — the new-create draft channel is left
  // untouched so an in-progress new song survives an edit detour.
  const [saved] = useState(() => (editGiftId ? null : loadDraft(lang)));

  // ── Draft state: the single source of truth for both the manual editor
  // and the AI collaboration panel. ──────────────────────────────────────
  const [instrumental, setInstrumental] = useState(saved?.instrumental ?? false);
  const [vocalGender, setVocalGender] = useState<"female" | "male" | null>(saved?.vocalGender ?? null);
  const [showMoreOptions, setShowMoreOptions] = useState(false);
  const [lyrics, setLyrics] = useState(saved?.lyrics ?? "");
  const [selectedStyles, setSelectedStyles] = useState<string[]>(saved?.selectedStyles ?? []);
  const [styleInput, setStyleInput] = useState(saved?.styleInput ?? "");
  const [title, setTitle] = useState(saved?.title ?? "");
  const [suggestions, setSuggestions] = useState<string[]>(() => shuffleStyles(saved?.selectedStyles ?? [], 14));
  const [error, setError] = useState<string | null>(null);

  // ── Edit-mode state (only used when editGiftId is set). ──────────────
  /** Load phase of the gift being edited; "readonly" = loaded but not owned. */
  const [editPhase, setEditPhase] = useState<"loading" | "ready" | "failed" | "readonly">(editGiftId ? "loading" : "ready");
  const [editError, setEditError] = useState<string | null>(null);
  const [gift, setGift] = useState<Gift | null>(null);
  const [versions, setVersions] = useState<GiftVersion[]>([]);
  /** Index into `versions` (newest first); the displayed audio/lyrics. */
  const [versionIdx, setVersionIdx] = useState(0);
  /** Regeneration progress for the edit loop, watched over SSE. */
  const [regen, setRegen] = useState<"idle" | "generating" | "error">("idle");
  /** True briefly after a title-only save lands. */
  const [savedFlash, setSavedFlash] = useState(false);
  /** True after a 409 — a job is already in flight server-side. */
  const [conflict, setConflict] = useState(false);
  const [playTime, setPlayTime] = useState(0);
  const editWatchRef = useRef<GenerationWatch | null>(null);
  const savedTimerRef = useRef<number | null>(null);

  // ── AI collaboration state (chat history intentionally not persisted). ──
  const [messages, setMessages] = useState<StudioMessage[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [chatError, setChatError] = useState<string | null>(null);
  const [flashed, setFlashed] = useState<Set<StudioField>>(new Set());
  const [mobileTab, setMobileTab] = useState<"draft" | "ai">("draft");

  // The undo stack lives in a ref (push happens inside the async chat loop);
  // undoCount is the render-facing mirror that drives the button's disabled
  // state. Restoring inside a setState updater would double-fire under
  // StrictMode.
  const undoRef = useRef<DraftSnapshot[]>([]);
  const [undoCount, setUndoCount] = useState(0);

  const bottomRef = useRef<HTMLDivElement>(null);
  const flashTimer = useRef<number | null>(null);

  // The async chat loop closes over the render it started in; this ref always
  // holds the latest committed draft so snapshots and request bodies are
  // never stale (same pattern as useGuidedState).
  const draftRef = useRef<DraftSnapshot>({ lyrics: "", selectedStyles: [], styleInput: "", vocalGender: null, instrumental: false, title: "" });
  draftRef.current = { lyrics, selectedStyles, styleInput, vocalGender, instrumental, title };

  // Persist the draft (new-create mode) on every change; chat history is
  // deliberately excluded per spec. Edit mode skips this so the new-create
  // draft survives an edit detour.
  useEffect(() => {
    if (editGiftId) return;
    const d = draftRef.current;
    try { sessionStorage.setItem(DRAFT_KEY, JSON.stringify({ lang, ...d })); } catch { /* quota */ }
  }, [editGiftId, lang, lyrics, selectedStyles, styleInput, vocalGender, instrumental, title]);

  // ── Edit mode: load the gift + its versions, fill the draft. ──────────
  useEffect(() => {
    if (!editGiftId) return;
    let cancelled = false;
    const token = creatorToken(editGiftId) ?? undefined;
    (async () => {
      try {
        const g = await getGift(editGiftId);
        if (cancelled) return;
        // Same ownership rule as GiftPage: device token or a session whose
        // creator_id matches. A non-owner gets a read-only notice instead of
        // edit controls (the API would 403 the submit anyway).
        const owned = token !== undefined || (user !== null && g.creator_id === user.id);
        if (!owned) {
          setGift(g);
          setEditPhase("readonly");
          return;
        }
        setGift(g);
        applyGiftToDraft(g);
        setEditPhase("ready");
        try {
          const v = await getGiftVersions(editGiftId, token);
          if (!cancelled) { setVersions(v); setVersionIdx(0); }
        } catch {
          // Versions are optional — the player falls back to the gift row.
        }
      } catch (e) {
        if (!cancelled) {
          setEditError(e instanceof Error ? e.message : "Failed to load gift");
          setEditPhase("failed");
        }
      }
    })();
    return () => { cancelled = true; editWatchRef.current?.close(); };
  }, [editGiftId, user]);

  /** Fill the draft editor from a gift's current work fields. */
  function applyGiftToDraft(g: Gift) {
    setLyrics(g.lyrics ?? "");
    setSelectedStyles([]);
    setStyleInput(g.meta.style ?? "");
    setInstrumental(g.meta.vocal === "instrumental");
    setVocalGender(g.meta.vocal === "male" ? "male" : g.meta.vocal === "female" ? "female" : null);
    setTitle(g.meta.title ?? "");
  }

  function flashSaved() {
    setSavedFlash(true);
    if (savedTimerRef.current) window.clearTimeout(savedTimerRef.current);
    savedTimerRef.current = window.setTimeout(() => setSavedFlash(false), FLASH_MS);
  }

  /** Light edit: title only — PATCH, show a saved confirmation, never
   *  trigger a generation. */
  async function handleSaveTitle() {
    if (!editGiftId || regen === "generating") return;
    setEditError(null); setConflict(false);
    try {
      const g = await updateGift(editGiftId, { title: title.trim() }, creatorToken(editGiftId) ?? undefined);
      setGift(g);
      flashSaved();
    } catch (e) {
      setEditError(e instanceof Error ? e.message : "Save failed");
    }
  }

  /** Heavy edit: lyrics/style/vocal changed — PATCH the work fields, then
   *  regenerate in place and watch the job to its terminal state. */
  async function handleSaveRegenerate() {
    if (!editGiftId || regen === "generating") return;
    if (!instrumental && !lyrics.trim()) return;
    setEditError(null); setConflict(false);
    const token = creatorToken(editGiftId) ?? undefined;
    const style = selectedStyles.join(", ") || styleInput.trim();
    try {
      const g = await updateGift(editGiftId, {
        lyrics: instrumental ? "" : lyrics.trim(),
        // Empty style stays untouched server-side: PATCH stores strings
        // verbatim, and "" would clobber meta.style past the provider's
        // fallback instead of meaning "cleared".
        style: style || undefined,
        title: title.trim(),
        vocal: instrumental ? "instrumental" : vocalGender ?? undefined,
      }, token);
      setGift(g);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : "Save failed");
      return;
    }
    try {
      await regenerateGift(editGiftId, token);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Regenerate failed";
      // 409: a job is already in flight — friendly hint instead of a failure.
      if (/: 409$/.test(msg)) setConflict(true); else setEditError(msg);
      return;
    }
    setRegen("generating");
    editWatchRef.current?.close();
    editWatchRef.current = watchGeneration(editGiftId, {
      onDone: () => {
        setRegen("idle");
        // New version landed — refresh the gift mirror and the version list
        // (default selection jumps back to the newest).
        void getGift(editGiftId).then(setGift).catch(() => undefined);
        void getGiftVersions(editGiftId, token).then(v => { setVersions(v); setVersionIdx(0); }).catch(() => undefined);
      },
      onFailed: (reason) => {
        setRegen("error");
        setEditError(
          reason === "timeout"
            ? "Generation timed out. Try again."
            : reason === "connection-lost"
              ? "Connection lost during generation"
              : "Music generation failed",
        );
      },
    });
  }

  /** "载入到草稿": copy a past version's work fields into the editor so the
   *  next save-and-regenerate branches off it. */
  function loadVersionToDraft(v: GiftVersion) {
    setLyrics(v.lyrics ?? "");
    setSelectedStyles([]);
    setStyleInput(v.meta.style ?? "");
    setInstrumental(v.meta.vocal === "instrumental");
    setVocalGender(v.meta.vocal === "male" ? "male" : v.meta.vocal === "female" ? "female" : null);
    setTitle(v.meta.title ?? "");
  }

  const vocal = !instrumental ? vocalGender || undefined : undefined;

  function addStyle(s: string) { if (s && !selectedStyles.includes(s)) setSelectedStyles(p => [...p, s]); }
  function removeStyle(s: string) { setSelectedStyles(p => p.filter(x => x !== s)); }
  function refreshSuggestions() { setSuggestions(shuffleStyles(selectedStyles, 14)); }
  function commitStyleInput() { const t = styleInput.trim(); if (t) addStyle(t); setStyleInput(""); }

  // ── AI collaboration ──────────────────────────────────────────────────

  /** The draft as the backend expects it: combined style, absent = unset. */
  function draftForApi(): NonNullable<Parameters<typeof streamChat>[0]["draft"]> {
    const d = draftRef.current;
    const style = d.selectedStyles.join(", ") || d.styleInput.trim();
    return {
      lyrics: d.lyrics.trim() || undefined,
      style: style || undefined,
      title: d.title.trim() || undefined,
      vocal: d.instrumental ? "instrumental" : d.vocalGender ?? undefined,
    };
  }

  function pushUndo(snapshot: DraftSnapshot) {
    undoRef.current = [...undoRef.current.slice(-(UNDO_CAP - 1)), snapshot];
    setUndoCount(undoRef.current.length);
  }

  function handleUndo() {
    const prev = undoRef.current[undoRef.current.length - 1];
    if (!prev) return;
    undoRef.current = undoRef.current.slice(0, -1);
    setUndoCount(undoRef.current.length);
    setLyrics(prev.lyrics);
    setSelectedStyles(prev.selectedStyles);
    setStyleInput(prev.styleInput);
    setVocalGender(prev.vocalGender);
    setInstrumental(prev.instrumental);
    setTitle(prev.title);
  }

  function flashFields(fields: StudioField[]) {
    setFlashed(new Set(fields));
    if (flashTimer.current) window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlashed(new Set()), FLASH_MS);
  }

  /**
   * Apply the non-null Done fields to the draft. The pre-change snapshot is
   * pushed BEFORE applying; the changed fields flash AFTER. Purely
   * conversational turns (all null) touch neither the draft nor the stack.
   * Returns the applied field names for the chat confirmation.
   */
  function applyDone(e: DoneEvent): StudioField[] {
    const fields: StudioField[] = [];
    if (e.lyrics != null) fields.push("lyrics");
    if (e.style != null) fields.push("style");
    if (e.title != null) fields.push("title");
    if (e.vocal != null) fields.push("vocal");
    if (!fields.length) return fields;
    pushUndo(draftRef.current);
    if (e.lyrics != null) setLyrics(e.lyrics);
    // An AI style replaces the manual chip selection: the backend combines
    // selectedStyles + styleInput the same way handleGenerate does, so the
    // returned style goes into styleInput and the chips clear.
    if (e.style != null) { setStyleInput(e.style); setSelectedStyles([]); }
    if (e.title != null) setTitle(e.title);
    if (e.vocal != null) { setVocalGender(e.vocal === "male" ? "male" : "female"); setInstrumental(false); }
    flashFields(fields);
    return fields;
  }

  /** Scroll the bubble list to the end. The instant variant only fires when
   * already near the bottom, so a user who scrolled up to read isn't yanked
   * back down (same contract as the guided flow). */
  function scrollToBottom(behavior: ScrollBehavior) {
    const el = bottomRef.current;
    const scroller = el?.parentElement;
    if (!el || !scroller) return;
    if (behavior === "auto" && scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight > 160) return;
    el.scrollIntoView({ behavior, block: "end" });
  }

  async function sendTurn(userText: string) {
    // The transcript always opens on a user turn: studio messages only ever
    // come from this handler, so no synthetic opener is needed.
    const msgs: StudioMessage[] = [...messages, { role: "user", content: userText }];
    setMessages(msgs);
    setStreaming(true); setChatError(null);
    let arrived = "";

    try {
      const gen = streamChat({ mode: "studio", draft: draftForApi(), messages: msgs, meta: { lang }, photos: [] });
      for await (const e of gen) {
        if (e.type === "Delta") {
          // Direct append — no typewriter reveal. Guided's rAF reveal exists
          // for long lyrics streams; studio bubbles are short text.
          arrived += e.text;
          setMessages([...msgs, { role: "assistant", content: arrived }]);
          scrollToBottom("auto");
        } else if (e.type === "Done") {
          const applied = applyDone(e);
          const note = applied.length
            ? t("applied_fields", { fields: applied.map(f => t(`field_${f}`)).join(t("field_sep")) })
            : undefined;
          setMessages([...msgs, { role: "assistant", content: arrived, note }]);
        } else if (e.type === "Error") {
          setMessages([...msgs, { role: "assistant", content: arrived }]);
          setChatError(e.error);
        }
      }
    } catch (err) {
      setChatError(err instanceof Error ? err.message : "Chat failed");
    } finally {
      setStreaming(false);
    }
  }

  async function handleChatSend() {
    const text = input.trim(); if (!text || streaming) return;
    setInput(""); await sendTurn(text);
  }

  // ── Generation (unchanged from the old free-create flow) ──────────────

  async function handleGenerate(e: FormEvent) {
    e.preventDefault(); setError(null);
    // Vocal mode requires lyrics — never substitute a placeholder: Suno
    // would sing the word "instrumental" as if it were the lyrics.
    if (!instrumental && !lyrics.trim()) return;
    const style = selectedStyles.join(", ") || styleInput.trim() || "warm acoustic";
    // Branch on the returned result: `gen.error` here would be the stale
    // closure from this render, always the pre-start value.
    const result = await gen.start({ lyrics: instrumental ? "" : lyrics.trim(), kind: instrumental ? "instrumental" : "song", style, title: title.trim() || undefined, vocal, lang, photos });
    if (!result.ok) setError(result.error);
  }

  /** Vocal mode with no lyrics yet: submission is blocked (see handleGenerate). */
  const needsLyrics = !instrumental && !lyrics.trim();

  const musicState = gen.state === "idle" ? "generating" as const : gen.state === "ready" ? "ready" as const : gen.state === "error" ? "error" as const : "generating" as const;

  // ── Chat display derivations ──────────────────────────────────────────

  // Auto-scroll on new bubbles
  const lastCount = useRef(0);
  if (messages.length !== lastCount.current) { lastCount.current = messages.length; setTimeout(() => scrollToBottom("smooth"), 50); }

  const lastMsg = messages[messages.length - 1];
  const lastAssistant = lastMsg && lastMsg.role === "assistant" ? lastMsg : null;
  const lastTurnText = lastAssistant
    ? stripMarkers(lastAssistant.content.split("<<<LYRICS>>>")[0]).trim()
    : "";
  const lyricsStreaming = streaming && !!lastAssistant && lastAssistant.content.includes("<<<LYRICS>>>");

  const flash = (f: StudioField) => (flashed.has(f) ? " ai-flash" : "");

  // ── Edit-mode display derivations: the selected version mirrors the
  // gift row for the latest, so both paths read the same shape. ──────────
  const shownVersion = editGiftId && versions.length > 0
    ? versions[Math.min(versionIdx, versions.length - 1)]
    : null;
  const shownAudio = shownVersion ? shownVersion.audio_url : gift?.audio_url ?? null;
  const shownLyrics = shownVersion ? shownVersion.lyrics : gift?.lyrics ?? null;
  const shownCover = shownVersion ? shownVersion.cover_url : gift?.cover_url ?? null;
  const shownTitle = (shownVersion ? shownVersion.meta.title : gift?.meta.title) ?? undefined;
  const shownLrc = shownVersion ? shownVersion.lrc : gift?.lrc ?? null;
  const shownLrcLines = shownLrc ? parseLRC(shownLrc) : null;

  if (editGiftId && editPhase === "loading") {
    return <div className="studio loading-page"><span className="spinner" /> {t("loading_gift")}</div>;
  }
  if (editGiftId && editPhase === "failed") {
    return <div className="studio"><div className="error-msg">{editError}</div></div>;
  }
  if (editGiftId && editPhase === "readonly") {
    return (
      <div className="studio">
        <div className="free-panel editorial">
          <p className="degraded-note">{t("edit_readonly")}</p>
          {gift?.audio_url && <AudioPlayer src={gift.audio_url} title={gift.meta.title ?? undefined} />}
        </div>
      </div>
    );
  }

  return (
    <div className="studio">
      {/* Mobile-only tab switcher (hidden ≥900px by CSS) */}
      <div className="studio-mobile-tabs">
        <button className={`tab-btn ${mobileTab === "draft" ? "active" : ""}`} onClick={() => setMobileTab("draft")}>{t("studio_draft_tab")}</button>
        <button className={`tab-btn ${mobileTab === "ai" ? "active" : ""}`} onClick={() => setMobileTab("ai")}>{t("studio_ai_tab")}</button>
      </div>

      <div className="studio-cols">
        {/* ═══ Left: draft editor ═══ */}
        <div className={`studio-draft-col${mobileTab === "draft" ? " m-active" : ""}`}>
          <div className="free-panel editorial">
            {/* Edit mode: current version player + version switcher. While a
                regeneration is in flight the old audio is gone server-side —
                show the generating state instead of a broken player. */}
            {editGiftId && (
              <div className="edit-head">
                {regen === "generating" ? (
                  <div className="polish-status"><span className="spinner" /> {t("generating")}</div>
                ) : shownAudio ? (
                  <>
                    {shownCover && <img className="edit-cover" src={shownCover} alt="" />}
                    <AudioPlayer key={shownAudio} src={shownAudio} title={shownTitle} onTimeUpdate={setPlayTime} />
                  </>
                ) : null}
                {versions.length > 1 && (
                  <div className="version-bar">
                    <span className="version-label">{t("versions")}</span>
                    {versions.map((v, i) => (
                      <button
                        key={v.version}
                        className={`version-pill${i === versionIdx ? " active" : ""}`}
                        onClick={() => { setVersionIdx(i); setPlayTime(0); }}
                      >
                        V{v.version}{i === 0 ? ` · ${t("version_latest")}` : ""}
                      </button>
                    ))}
                    <button className="version-load" onClick={() => loadVersionToDraft(versions[versionIdx])}>
                      {t("load_to_draft")}
                    </button>
                  </div>
                )}
                {regen !== "generating" && (shownLrcLines?.length ? (
                  <div className="edit-lyrics">
                    <LRCViewer lines={shownLrcLines} currentTime={playTime} onSeek={(time) => {
                      const audio = document.querySelector("audio");
                      if (audio) audio.currentTime = time;
                    }} />
                  </div>
                ) : shownLyrics ? (
                  <div className="edit-lyrics">{shownLyrics}</div>
                ) : null)}
              </div>
            )}
            {/* Vocal / Instrumental */}
            <div className={`mode-bar${flash("vocal")}`}>
              <button className={`mode-btn ${!instrumental ? "active" : ""}`} onClick={() => setInstrumental(false)}>
                <span className="mode-icon"><MicIcon /></span>
                <span className="mode-label">{t("vocal")}</span>
              </button>
              <button className={`mode-btn ${instrumental ? "active" : ""}`} onClick={() => setInstrumental(true)}>
                <span className="mode-icon"><MusicNoteIcon /></span>
                <span className="mode-label">{t("instrumental")}</span>
              </button>
            </div>

            {/* Lyrics */}
            <section className={`editorial-section${flash("lyrics")}`}>
              <h3 className="section-title">{t("free_lyrics")}</h3>
              <textarea className="lyrics-manuscript" value={lyrics} onChange={e => setLyrics(e.target.value)}
                placeholder={instrumental ? t("instrumental_ph") : t("paste_lyrics_ph")}
                disabled={instrumental} rows={instrumental ? 2 : 7} />
            </section>

            {/* Style */}
            <section className={`editorial-section${flash("style")}`}>
              <h3 className="section-title">{t("free_style")}</h3>
              <div className="style-composer">
                <input type="text" className="style-input" value={styleInput} onChange={e => setStyleInput(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter" && !isImeComposing(e)) { e.preventDefault(); commitStyleInput(); } else if (e.key === "Backspace" && !styleInput && selectedStyles.length > 0) removeStyle(selectedStyles[selectedStyles.length - 1]); }}
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
                      {t("more_options")}
                    </button>
                    <div className="t-acc-panel">
                      <div className="more-body t-acc-panel-inner">
                        <p className="more-label">{t("vocal_gender")}</p>
                        <div className="gender-select">
                          <button className={`gender-opt ${vocalGender === "female" ? "on" : ""}`} onClick={() => setVocalGender(vocalGender === "female" ? null : "female")}><FemaleIcon /> {t("gender_female")}</button>
                          <button className={`gender-opt ${vocalGender === "male" ? "on" : ""}`} onClick={() => setVocalGender(vocalGender === "male" ? null : "male")}><MaleIcon /> {t("gender_male")}</button>
                        </div>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </section>

            {/* Title */}
            <section className={`editorial-section${flash("title")}`}>
              <h3 className="section-title">{t("free_title")}</h3>
              <input type="text" className="title-input" value={title} onChange={e => setTitle(e.target.value)} placeholder={t("free_title_ph")} maxLength={50} />
            </section>

            {needsLyrics && <p className="polish-status">{t("lyrics_required")}</p>}
            {editGiftId ? (
              <>
                {/* Two explicit save paths: light (title only, no generation)
                    and heavy (work fields + regenerate in place). */}
                <div className="edit-actions">
                  <button className="btn btn-secondary" onClick={() => void handleSaveTitle()} disabled={regen === "generating"}>
                    {savedFlash ? t("saved") : t("save")}
                  </button>
                  <button className="btn-create" onClick={() => void handleSaveRegenerate()} disabled={regen === "generating" || needsLyrics}>
                    {regen === "generating" ? <><span className="spinner" /> {t("generating")}</> : t("save_and_regenerate")}
                  </button>
                </div>
                {conflict && <p className="polish-status">{t("regen_conflict")}</p>}
                {editError && <p className="error-msg" role="alert">{editError}</p>}
              </>
            ) : (
              <>
                <button className="btn-create" onClick={handleGenerate} disabled={gen.state === "generating" || needsLyrics}>
                  {gen.state === "generating" ? <><span className="spinner" /> {t("generating")}</> : t("create_song")}
                </button>

                {(error || gen.error) && <p className="error-msg" role="alert">{error || gen.error}</p>}
                {gen.giftId && <MusicCard initialState={musicState} onOpen={() => onNavigate(gen.giftId!)} onRetry={() => gen.retry(gen.giftId!)} />}
              </>
            )}
          </div>
        </div>

        {/* ═══ Right: AI collaboration panel ═══ */}
        <div className={`studio-ai-col${mobileTab === "ai" ? " m-active" : ""}`}>
          <div className="chat-panel">
            <div className="studio-ai-toolbar">
              <span className="studio-ai-title">{t("studio_ai_tab")}</span>
              <button className="undo-btn" onClick={handleUndo} disabled={undoCount === 0} title={t("undo")} aria-label={t("undo")}>↩</button>
            </div>
            <div className="chat-messages guided">
              {messages.length === 0 && <div className="bubble bot">{t("studio_ai_intro")}</div>}

              {messages.map((msg, i) => {
                // Stop at <<<LYRICS>>> — marker blocks belong to the draft,
                // not the chat bubble (same truncation as the guided flow).
                const display = msg.role === "assistant"
                  ? stripMarkers(msg.content.split("<<<LYRICS>>>")[0])
                  : stripMarkers(msg.content);
                if (!display.trim() && !msg.note) return null;
                return (
                  <div key={`m-${i}`} className={`bubble ${msg.role === "assistant" ? "bot" : msg.role}`}>
                    {display}
                    {msg.note && <span className="applied-note">{msg.note}</span>}
                  </div>
                );
              })}

              {/* Typing indicator: waiting for the first visible text of the
                  current assistant turn. */}
              {streaming && !lyricsStreaming && !lastTurnText && (
                <div className="bubble bot typing-indicator" aria-label="Assistant is typing">
                  <span className="typing-dot" />
                  <span className="typing-dot" />
                  <span className="typing-dot" />
                </div>
              )}

              {/* The hidden <<<LYRICS>>> block is streaming into the draft. */}
              {streaming && lyricsStreaming && (
                <div className="bubble bot reviewing-indicator" role="status">
                  <span className="typing-dot" />
                  <span className="typing-dot" />
                  <span className="typing-dot" />
                  <span className="reviewing-label">{t("writing_lyrics")}</span>
                </div>
              )}

              <div ref={bottomRef} />
            </div>
            {chatError && <div className="error-msg" style={{ margin: "0 16px 8px" }}>{chatError}</div>}
            <div className="chat-bar">
              <textarea value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !isImeComposing(e)) { e.preventDefault(); handleChatSend(); } }} rows={1} disabled={streaming} placeholder={t("ai_placeholder")} />
              <button className="chat-send-btn" onClick={handleChatSend} disabled={streaming} aria-label="Send"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M2 21l21-9L2 3v7l15 2-15 2z" /></svg></button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
