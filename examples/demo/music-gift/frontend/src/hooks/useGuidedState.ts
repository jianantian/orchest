import { useRef, useState } from "react";
import type { ChatMessage } from "../types";

/**
 * A guided-flow turn. `hidden` marks the synthetic opening turn: it must stay
 * in the transcript so the API always sees a user-first history, but it is
 * never rendered. The backend ignores the extra field.
 */
export interface GuidedMessage extends ChatMessage {
  hidden?: boolean;
}

export type FlowStep = "greet" | "relationship" | "name" | "gender" | "birthday" | "scenario" | "chat" | "review" | "music";

export interface StepMeta {
  relationship: string; relationshipLabel: string;
  name: string; gender: string;
  birthday: { month: number; day: number } | null;
  scenario: string; scenarioLabel: string;
}

/** The lyrics the review card edits, as parsed by the backend. */
export interface LyricsDraft {
  lyrics: string; style: string; title: string; vocal: string;
}

interface Saved {
  lang: string; step: FlowStep; meta: StepMeta; messages: GuidedMessage[];
  draft: LyricsDraft | null;
  /** Gift created by the music step — persisted so a remount (back from the
   * gift page) can restore the MusicCard instead of reverting to review. */
  giftId: string | null;
  /** Last known generation status for giftId. */
  musicState: "generating" | "ready" | "error" | null;
}

const KEY = "moment_guided";

const EMPTY_META: StepMeta = {
  relationship: "", relationshipLabel: "", name: "", gender: "",
  birthday: null, scenario: "", scenarioLabel: "",
};

function load(): Saved | null {
  try { const v = sessionStorage.getItem(KEY); return v ? JSON.parse(v) : null; } catch { return null; }
}
function save(s: Saved) {
  try { sessionStorage.setItem(KEY, JSON.stringify(s)); } catch { /* quota */ }
}
export function clearGuided() {
  try { sessionStorage.removeItem(KEY); } catch { /* ignore */ }
}

export function useGuidedState(lang: string) {
  // Read sessionStorage once per mount, not on every render.
  const [snapshot] = useState(() => {
    const r = load();
    if (!r || r.lang !== lang) return null;
    // Snapshots written before gift persistence existed lack these fields.
    r.giftId ??= null;
    r.musicState ??= null;
    // "music" is only restorable with a persisted giftId — without it the
    // MusicCard can't render and the chat bar would be dead. Fall back to
    // "review" — the draft is persisted, so generation can be re-submitted.
    if (r.step === "music" && !r.giftId) r.step = "review";
    return r;
  });

  const [step, setStep] = useState<FlowStep>(snapshot?.step ?? "greet");
  const [meta, rawSetMeta] = useState<StepMeta>(snapshot?.meta ?? EMPTY_META);
  const [messages, setMessages] = useState<GuidedMessage[]>(snapshot?.messages ?? []);
  const [draft, rawSetDraft] = useState<LyricsDraft | null>(snapshot?.draft ?? null);
  const [giftId, rawSetGiftId] = useState<string | null>(snapshot?.giftId ?? null);
  const [musicState, rawSetMusicState] = useState<Saved["musicState"]>(snapshot?.musicState ?? null);

  // persist() must never read the state variables directly: the async chat
  // loop holds a closure from the render it started in, so a go() at the end
  // of a turn would write back a pre-turn transcript. These refs always hold
  // the latest committed values.
  const metaRef = useRef(meta); metaRef.current = meta;
  const msgRef = useRef(messages); msgRef.current = messages;
  const draftRef = useRef(draft); draftRef.current = draft;
  const stepRef = useRef(step); stepRef.current = step;
  const giftIdRef = useRef(giftId); giftIdRef.current = giftId;
  const musicStateRef = useRef(musicState); musicStateRef.current = musicState;

  const lastSave = useRef(0);
  function persist(s: FlowStep = stepRef.current) {
    save({
      lang, step: s, meta: metaRef.current,
      messages: msgRef.current.slice(-20), draft: draftRef.current,
      giftId: giftIdRef.current, musicState: musicStateRef.current,
    });
    lastSave.current = Date.now();
  }

  function go(s: FlowStep) { setStep(s); stepRef.current = s; persist(s); }
  function setMeta(m: StepMeta) { rawSetMeta(m); metaRef.current = m; persist(); }
  function setMsg(ms: GuidedMessage[]) { setMessages(ms); msgRef.current = ms; persist(); }
  /**
   * Streaming variant of setMsg: state and refs update every call (so the UI
   * and any later persist see them), but the sessionStorage write is
   * throttled. Stringifying the whole transcript per token was O(n²) and
   * visibly stuttered the stream. The caller must end the turn with a real
   * setMsg / go so the final state is persisted.
   */
  function setMsgStreaming(ms: GuidedMessage[]) {
    setMessages(ms); msgRef.current = ms;
    if (Date.now() - lastSave.current >= 500) persist();
  }
  function setDraft(d: LyricsDraft | null) { rawSetDraft(d); draftRef.current = d; persist(); }
  /** Record that generation started for a gift: id known, still cooking. */
  function setGift(id: string) {
    rawSetGiftId(id); giftIdRef.current = id;
    rawSetMusicState("generating"); musicStateRef.current = "generating";
    persist();
  }
  /** Record a terminal generation status ("ready" / "error"). */
  function setMusicState(s: "ready" | "error") {
    rawSetMusicState(s); musicStateRef.current = s; persist();
  }

  /** Discard the whole session and return to the first question. */
  function reset() {
    clearGuided();
    setStep("greet"); stepRef.current = "greet";
    rawSetMeta(EMPTY_META); metaRef.current = EMPTY_META;
    setMessages([]); msgRef.current = [];
    rawSetDraft(null); draftRef.current = null;
    rawSetGiftId(null); giftIdRef.current = null;
    rawSetMusicState(null); musicStateRef.current = null;
  }

  return {
    step, meta, messages, draft, metaRef, giftId, musicState,
    actions: { go, setMeta, setMsg, setMsgStreaming, setDraft, setGift, setMusicState, reset },
    wasRestored: !!snapshot,
  };
}
