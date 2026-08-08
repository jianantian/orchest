import { useEffect, useRef, useState, type FormEvent, type SyntheticEvent } from "react";
import { streamChat, getGift, updateGift, regenerateGift, getGiftVersions, watchGeneration, type GenerationWatch } from "../api";
import { useI18n } from "../i18n";
import { shuffleStyles, stripMarkers } from "../lib/styles";
import { lineRangeForSelection, splitLines, spliceLines } from "../lib/lyrics";
import { isImeComposing } from "../lib/ime";
import { creatorToken } from "../lib/creator";
import { useAuth } from "../hooks/useAuth";
import { useMusicGen } from "../hooks/useMusicGen";
import { MusicCard } from "./MusicCard";
import AudioPlayer from "./AudioPlayer";
import { LRCViewer } from "./LRCViewer";
import { parseLRC } from "../lib/lrc";
import { SparklesIcon } from "./Icons";
import { StyleCard } from "./studio/StyleCard";
import { PlayerCard } from "./studio/PlayerCard";
import { TakesCard } from "./studio/TakesCard";
import { SelectionToolbar, type ScopedCommand } from "./studio/SelectionToolbar";
import { LyricsProposal, type Proposal } from "./studio/LyricsProposal";
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
  const [lyrics, setLyrics] = useState(saved?.lyrics ?? "");
  const [selectedStyles, setSelectedStyles] = useState<string[]>(saved?.selectedStyles ?? []);
  const [styleInput, setStyleInput] = useState(saved?.styleInput ?? "");
  const [title, setTitle] = useState(saved?.title ?? "");
  // Persistent style suggestions: 6 draws from the catalog, always visible
  // inside the 风格 section; the refresh button re-draws, excluding the
  // styles already selected.
  const [styleSuggestions, setStyleSuggestions] = useState<string[]>(() => shuffleStyles(selectedStyles, 6));
  const [error, setError] = useState<string | null>(null);
  // New-create artifact: the finished gift, fetched once the watch reports
  // ready so the artifact column can show the player in place instead of
  // navigating away to the gift page.
  const [newGift, setNewGift] = useState<Gift | null>(null);

  // ── AI region collapse state (deliberately not persisted, per PRD). The
  // draft cards themselves are always expanded in the two-column workbench
  // layout — the old DraftSection collapse is retired. ──────────────────
  const [aiOpen, setAiOpen] = useState(true);

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

  // ── Manuscript selection state: the 1-based closed line range covering
  // the textarea selection (Task 8/C4 consume this too), plus the toolbar's
  // estimated vertical offset inside .wb-manuscript. ──────────────────────
  const [selRange, setSelRange] = useState<{ from: number; to: number } | null>(null);
  const [selTop, setSelTop] = useState(0);
  const manuscriptRef = useRef<HTMLTextAreaElement>(null);
  const docCardRef = useRef<HTMLDivElement>(null);
  const overlayInnerRef = useRef<HTMLDivElement>(null);
  const selBlurTimer = useRef<number | null>(null);

  // ── AI proposal state: an inline lyric diff pending accept/reject,
  // rendered in the manuscript in place of lines from–to (Task 10 fills
  // this from Done.lines). While set, the manuscript swaps the textarea
  // for a read-only line view and the selection UI is suppressed. ─────
  const [proposal, setProposal] = useState<Proposal | null>(null);

  // The undo stack lives in a ref (push happens inside the async chat loop);
  // undoCount is the render-facing mirror that drives the button's disabled
  // state. Restoring inside a setState updater would double-fire under
  // StrictMode.
  const undoRef = useRef<DraftSnapshot[]>([]);
  const [undoCount, setUndoCount] = useState(0);

  const bottomRef = useRef<HTMLDivElement>(null);
  const flashTimer = useRef<number | null>(null);
  /** The whole AI region — the ✨ auto-send scrolls it into view. */
  const aiRegionRef = useRef<HTMLDivElement>(null);

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

  // New-create mode: when the generation watch lands on ready, fetch the
  // finished gift for the artifact-column player. Cleared again as soon as
  // the state leaves ready (retry / reset / a fresh run).
  useEffect(() => {
    if (editGiftId || gen.state !== "ready" || !gen.giftId) { setNewGift(null); return; }
    let cancelled = false;
    void getGift(gen.giftId).then(g => { if (!cancelled) setNewGift(g); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [editGiftId, gen.state, gen.giftId]);

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
  function commitStyleInput() { const t = styleInput.trim(); if (t) addStyle(t); setStyleInput(""); }
  /** ↻ re-draw the persistent suggestion row, excluding selected styles. */
  function refreshSuggestions() { setStyleSuggestions(shuffleStyles(selectedStyles, 6)); }

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
    // AI vocal maps onto the tri-state: male/female select that pill (and
    // clear instrumental); "instrumental" selects 器乐.
    if (e.vocal != null) {
      if (e.vocal === "instrumental") { setInstrumental(true); setVocalGender(null); }
      else { setVocalGender(e.vocal === "male" ? "male" : "female"); setInstrumental(false); }
    }
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

  /** ✨ 帮写 / 帮我写歌词: expand the AI region if collapsed, scroll it into
   *  view, then auto-send a localized natural-language request through the
   *  normal chat turn path (no special backend handling needed). */
  function handleWriteForMe() {
    if (streaming) return;
    setAiOpen(true);
    // A frame later the region is expanded and measurable — scroll to it.
    requestAnimationFrame(() => aiRegionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" }));
    void sendTurn(t("ai_prompt_write_lyrics"));
  }

  // ── Manuscript selection → floating toolbar ───────────────────────────

  /** Toolbar offset from the top of .wb-manuscript: padding-top +
   *  (from-1) × line-height, adjusted for scroll, placed just above the
   *  first selected line (≈ toolbar height + arrow), clamped into view. */
  function computeSelTop(ta: HTMLTextAreaElement, from: number): number {
    const cs = getComputedStyle(ta);
    const lineHeight = parseFloat(cs.lineHeight) || 34;
    const padTop = parseFloat(cs.paddingTop) || 0;
    return Math.max(4, padTop + (from - 1) * lineHeight - ta.scrollTop - 44);
  }

  function handleLyricsSelect(e: SyntheticEvent<HTMLTextAreaElement>) {
    const ta = e.currentTarget;
    if (ta.selectionEnd > ta.selectionStart) {
      const { from, to } = lineRangeForSelection(ta.value, ta.selectionStart, ta.selectionEnd);
      setSelRange({ from, to });
      setSelTop(computeSelTop(ta, from));
    } else {
      setSelRange(null);
    }
  }

  /** The read-only highlight overlay doesn't scroll itself — mirror the
   *  textarea's scrollTop onto it, and keep the toolbar glued to the line. */
  function handleLyricsScroll(e: SyntheticEvent<HTMLTextAreaElement>) {
    const ta = e.currentTarget;
    if (overlayInnerRef.current) overlayInnerRef.current.style.transform = `translateY(${-ta.scrollTop}px)`;
    if (selRange) setSelTop(computeSelTop(ta, selRange.from));
  }

  // The overlay mounts after the selection is made; apply the current
  // scroll offset once so a mid-scroll selection aligns immediately.
  useEffect(() => {
    if (selRange && overlayInnerRef.current && manuscriptRef.current)
      overlayInnerRef.current.style.transform = `translateY(${-manuscriptRef.current.scrollTop}px)`;
  }, [selRange]);

  /** Clear the selection 150ms after focus leaves the manuscript card,
   *  unless focus landed on something inside the card (the delay lets a
   *  toolbar click land before the range is torn down). */
  function handleDocBlur() {
    if (selBlurTimer.current) window.clearTimeout(selBlurTimer.current);
    selBlurTimer.current = window.setTimeout(() => {
      if (!docCardRef.current?.contains(document.activeElement)) setSelRange(null);
    }, 150);
  }

  /** No AI request yet (Task 10 wires the scoped commands). "custom" routes
   *  to the chat input with the line range prefilled; the other commands
   *  just keep the selection highlighted as a visual anchor. */
  function handleScopedAction(cmd: ScopedCommand) {
    if (!selRange) return;
    if (cmd === "custom") {
      setAiOpen(true);
      setInput(t("scoped_custom_prefill", { from: selRange.from, to: selRange.to }));
      requestAnimationFrame(() => aiRegionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" }));
    }
  }

  // ── Inline diff proposal: accept applies the splice under an undo
  //  snapshot (same contract as applyDone); reject just drops it. ──────

  // A pending proposal is mutually exclusive with the selection overlay:
  // the textarea is unmounted in proposal state, so any stale line range
  // would float the toolbar over the read-only view.
  useEffect(() => {
    if (proposal) setSelRange(null);
  }, [proposal]);

  function acceptProposal() {
    if (!proposal) return;
    pushUndo(draftRef.current);
    setLyrics(spliceLines(lyrics, proposal.from, proposal.to, proposal.replacement));
    setProposal(null);
  }

  function rejectProposal() { setProposal(null); }

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
      {/* ═══ Two-column workbench: driver (manuscript + style + actions)
          left, artifacts right (edit mode: 试听卡 + 版本卡 above the AI
          chat panel). ═══ */}
      <div className="wb-cols">
        <div className="wb-col wb-col-driver">
        {/* 1–2. Manuscript card: borderless serif title (with the ✨ 帮写
            tool pinned to its row), divider, then the lyrics body with a
            "帮我写歌词" CTA when empty. */}
        <div className="wb-card wb-doc" ref={docCardRef} onBlur={handleDocBlur}>
          <div className="wb-doc-head">
            <input type="text" className={`title-input${flash("title")}`} value={title} onChange={e => setTitle(e.target.value)} placeholder={t("free_title_ph")} maxLength={50} />
            <button type="button" className="icon-btn" onClick={handleWriteForMe} title={t("ai_help_write")} aria-label={t("ai_help_write")}><SparklesIcon /></button>
          </div>
          <hr className="wb-doc-divider" />
          <div className="wb-manuscript">
            {proposal ? (
              /* Proposal state: read-only line view with the diff hunk in
                 place of lines from–to; the textarea (and with it the
                 selection UI) returns once the proposal is settled. */
              <div className="wb-manuscript-view">
                {splitLines(lyrics).map((line, i) => {
                  const n = i + 1;
                  if (n === proposal.from) {
                    return <LyricsProposal key="proposal" proposal={proposal} onAccept={acceptProposal} onReject={rejectProposal} />;
                  }
                  if (n > proposal.from && n <= proposal.to) return null;
                  return <p key={n} className="wb-line">{line || " "}</p>;
                })}
              </div>
            ) : (
              <textarea ref={manuscriptRef} className={`lyrics-manuscript${flash("lyrics")}`} value={lyrics} onChange={e => setLyrics(e.target.value)}
                onSelect={handleLyricsSelect} onScroll={handleLyricsScroll}
                placeholder={instrumental ? t("instrumental_ph") : t("paste_lyrics_ph")}
                disabled={instrumental} rows={instrumental ? 2 : 7} />
            )}
            {/* While a selection is active, a read-only line layer sits over
                the textarea (transparent text, .sel rows tinted) — the
                textarea has no per-line DOM to highlight directly. */}
            {selRange && !proposal && (
              <div className="wb-manuscript-overlay" aria-hidden="true">
                <div ref={overlayInnerRef}>
                  {splitLines(lyrics).map((line, i) => (
                    <p key={i} className={`wb-line${i + 1 >= selRange.from && i + 1 <= selRange.to ? " sel" : ""}`}>{line || " "}</p>
                  ))}
                </div>
              </div>
            )}
            {selRange && !proposal && (
              <div style={{ position: "absolute", top: selTop, left: 0, right: 0 }}>
                <SelectionToolbar onAction={handleScopedAction} />
              </div>
            )}
          </div>
          {!instrumental && !lyrics.trim() && (
            <button type="button" className="opt-pill write-for-me" onClick={handleWriteForMe}><SparklesIcon /> {t("write_for_me")}</button>
          )}
        </div>

        {/* 3–4. Style + vocal options card (merged, always expanded). */}
        <StyleCard
          selectedStyles={selectedStyles}
          styleInput={styleInput}
          onStyleInputChange={setStyleInput}
          styleSuggestions={styleSuggestions}
          instrumental={instrumental}
          vocalGender={vocalGender}
          onInstrumentalChange={setInstrumental}
          onVocalGenderChange={setVocalGender}
          onAddStyle={addStyle}
          onRemoveStyle={removeStyle}
          onCommitStyleInput={commitStyleInput}
          onRefreshSuggestions={refreshSuggestions}
          styleFlash={flash("style")}
          vocalFlash={flash("vocal")}
        />

        {/* 5. Action area */}
        {needsLyrics && <p className="polish-status">{t("lyrics_required")}</p>}
        {editGiftId ? (
          <>
            {/* Two explicit save paths: light (title only, no generation)
                and heavy (work fields + regenerate in place). */}
            <div className="wb-action-row edit-actions">
              <button className="btn btn-secondary" onClick={() => void handleSaveTitle()} disabled={regen === "generating"}>
                {savedFlash ? t("saved") : t("save")}
              </button>
              <button className="btn btn-primary" onClick={() => void handleSaveRegenerate()} disabled={regen === "generating" || needsLyrics}>
                {regen === "generating" ? <><span className="spinner" /> {t("generating")}</> : t("save_and_regenerate")}
              </button>
            </div>
            {conflict && <p className="polish-status">{t("regen_conflict")}</p>}
            {editError && <p className="error-msg" role="alert">{editError}</p>}
          </>
        ) : (
          <>
            <div className="wb-action-row">
              <button className="btn btn-primary btn-lg btn-full" onClick={handleGenerate} disabled={gen.state === "generating" || needsLyrics}>
                {gen.state === "generating" ? <><span className="spinner" /> {t("generating")}</> : t("create_song")}
              </button>
            </div>

            {(error || gen.error) && <p className="error-msg" role="alert">{error || gen.error}</p>}
          </>
        )}
        </div>

        <div className="wb-col wb-col-artifact">
      {/* Edit mode: 试听卡 + 版本卡. While a regeneration is in flight the
          old audio is gone server-side — PlayerCard shows the generating
          state instead of a broken player. */}
      {editGiftId && (
        <>
          <PlayerCard
            audioUrl={shownAudio}
            coverUrl={shownCover}
            title={shownTitle}
            versionLabel={shownVersion ? `V${shownVersion.version}${versionIdx === 0 ? ` · ${t("version_latest")}` : ""}` : ""}
            onTimeUpdate={setPlayTime}
            generating={regen === "generating"}
          >
            {shownLrcLines?.length ? (
              <div className="edit-lyrics">
                <LRCViewer lines={shownLrcLines} currentTime={playTime} onSeek={(time) => {
                  const audio = document.querySelector("audio");
                  if (audio) audio.currentTime = time;
                }} />
              </div>
            ) : shownLyrics ? (
              <div className="edit-lyrics">{shownLyrics}</div>
            ) : null}
          </PlayerCard>
          {versions.length > 1 && (
            <TakesCard
              versions={versions}
              currentIdx={versionIdx}
              onSelect={(i) => { setVersionIdx(i); setPlayTime(0); }}
              onBranch={(i) => loadVersionToDraft(versions[i])}
            />
          )}
        </>
      )}
      {/* New-create mode: the generation result lands in the artifact
          column in place — MusicCard progress while cooking, the player
          once ready, with the gift page kept as a secondary exit (no more
          auto-navigation, spec §10.9). */}
      {!editGiftId && gen.giftId && (
        gen.state === "ready" && newGift ? (
          <>
            <PlayerCard
              audioUrl={newGift.audio_url}
              coverUrl={newGift.cover_url ?? null}
              title={newGift.meta.title ?? undefined}
              versionLabel={`V1 · ${t("version_latest")}`}
              onTimeUpdate={setPlayTime}
            />
            <div className="wb-action-row">
              <button type="button" className="btn btn-secondary" onClick={() => onNavigate(gen.giftId!)}>
                {t("open_gift_page")}
              </button>
            </div>
          </>
        ) : (
          <MusicCard initialState={musicState} onOpen={() => onNavigate(gen.giftId!)} onRetry={() => gen.retry(gen.giftId!)} />
        )
      )}
      {/* ═══ AI collaboration region: collapsible — header holds the
          chevron + title + undo; collapsed hides bubbles and the pinned
          input, leaving the pure manual panel. ═══ */}
      <div className="chat-panel" ref={aiRegionRef}>
        <div className="studio-ai-toolbar">
          <button type="button" className="draft-section-toggle" onClick={() => setAiOpen(o => !o)} aria-expanded={aiOpen}>
            <svg className={`chevron${aiOpen ? " open" : ""}`} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6" /></svg>
            <span className="studio-ai-title">{t("studio_ai_tab")}</span>
          </button>
          <button className="icon-btn" onClick={handleUndo} disabled={undoCount === 0} title={t("undo")} aria-label={t("undo")}>↩</button>
        </div>
        {aiOpen && (<>
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
          {chatError && <div className="error-msg chat-error">{chatError}</div>}
          <div className="chat-bar">
            <textarea value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !isImeComposing(e)) { e.preventDefault(); handleChatSend(); } }} rows={1} disabled={streaming} placeholder={t("ai_placeholder")} />
            <button className="chat-send-btn" onClick={handleChatSend} disabled={streaming} aria-label="Send"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M2 21l21-9L2 3v7l15 2-15 2z" /></svg></button>
          </div>
        </>)}
      </div>
        </div>
      </div>
    </div>
  );
}
