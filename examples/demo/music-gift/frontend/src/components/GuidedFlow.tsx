import { useEffect, useRef, useState } from "react";
import { streamChat } from "../api";
import { useI18n, getMonths, getStyleTags } from "../i18n";
import { useMusicGen } from "../hooks/useMusicGen";
import { useGuidedState, clearGuided, type FlowStep, type GuidedMessage, type StepMeta } from "../hooks/useGuidedState";
import { ReviewCard, type ReviewData } from "./ReviewCard";
import { MusicCard } from "./MusicCard";
import { PillsRow, GoldPill, InlineInput, BirthdayPicker } from "./ChatUI";
import { stripMarkers } from "../lib/styles";
import { isImeComposing } from "../lib/ime";

/** The backend parses birthday as "MM-DD"; omitted entirely when skipped. */
function birthdayParam(b: StepMeta["birthday"]): string | undefined {
  return b ? `${b.month}-${b.day}` : undefined;
}

/** The scenario sent to the backend is the display label. For "__custom__"
 * ("tell me…") there is no preset angle, so it is omitted entirely — the
 * model must not read the pill label as an actual scenario. */
function scenarioParam(m: StepMeta): string | undefined {
  return m.scenario === "__custom__" ? undefined : m.scenarioLabel;
}

const RELATIONSHIPS = [
  { label: "rel_kid", value: "kid" }, { label: "rel_partner", value: "partner" },
  { label: "rel_friend", value: "friend" }, { label: "rel_parent", value: "parent" },
  { label: "rel_pet", value: "pet" }, { label: "rel_custom", value: "__custom__" },
];

function scenarioList(rel: string, t: (k: string) => string): Array<{ label: string; value: string }> {
  const base: Record<string, Array<{ key: string; value: string }>> = {
    kid: [
      { key: "scen_kid_birthday", value: "birthday" },
      { key: "scen_kid_graduation", value: "graduation" },
      { key: "scen_kid_daily", value: "daily_life" },
      { key: "scen_kid_encouragement", value: "encouragement" },
    ],
    partner: [
      { key: "scen_partner_anniversary", value: "anniversary" },
      { key: "scen_partner_date", value: "date_night" },
      { key: "scen_partner_apology", value: "apology" },
      { key: "scen_partner_justbecause", value: "just_because" },
    ],
    friend: [
      { key: "scen_friend_birthday", value: "birthday" },
      { key: "scen_friend_thanks", value: "thank_you" },
      { key: "scen_friend_joke", value: "inside_joke" },
      { key: "scen_friend_travel", value: "travel_memory" },
    ],
    parent: [
      { key: "scen_parent_birthday", value: "birthday" },
      { key: "scen_parent_mothersday", value: "mothers_day" },
      { key: "scen_parent_fathersday", value: "fathers_day" },
      { key: "scen_parent_gratitude", value: "gratitude" },
    ],
    pet: [
      { key: "scen_pet_gotcha", value: "gotcha_day" },
      { key: "scen_pet_joy", value: "daily_joy" },
      { key: "scen_pet_memorial", value: "memorial" },
    ],
    custom: [
      { key: "scen_custom_birthday", value: "birthday" },
      { key: "scen_custom_anniversary", value: "anniversary" },
      { key: "scen_custom_thanks", value: "thank_you" },
    ],
  };
  const items = base[rel] ?? base.custom;
  return [...items.map(({ key, value }) => ({ label: t(key), value })), { label: t("scenario_custom"), value: "__custom__" }];
}

// ── Derive bubbles from structured state ─────────────────

function derivedBubbles(step: FlowStep, meta: ReturnType<typeof useGuidedState>["meta"], t: (k: string) => string, months: string[]): Array<{ role: "bot" | "user"; text: string }> {
  const b: Array<{ role: "bot" | "user"; text: string }> = [];
  const push = (role: "bot" | "user", text: string) => { if (text) b.push({ role, text }); };

  if (step === "greet" || step === "relationship") {
    push("bot", t("greet"));
    if (step === "relationship") push("bot", t("relationship_q"));
    return b;
  }

  push("bot", t("greet"));
  push("bot", t("relationship_q"));
  push("user", meta.relationshipLabel);

  // Pets skip name/gender/birthday entirely — don't fabricate those Q&A
  // pairs into the transcript (they never happened).
  if (meta.relationship !== "pet") {
    push("bot", t("name_q"));
    if (step === "name") return b;
    push("user", meta.name);

    push("bot", t("gender_q"));
    if (step === "gender") return b;
    push("user", meta.gender);

    push("bot", meta.relationship === "partner" ? t("bday_q_partner") : t("bday_q"));
    if (step === "birthday") return b;
    push("user", meta.birthday ? `${months[meta.birthday.month - 1]} ${meta.birthday.day}` : t("bday_skip"));
  }

  push("bot", t("scenario_q"));
  if (step === "scenario") return b;
  push("user", meta.scenarioLabel);

  return b;
}

// ── Component ────────────────────────────────────────────

export function GuidedFlow({ onNavigate, onSwitchToFree }: { onNavigate: (giftId: string) => void; onSwitchToFree?: () => void }) {
  const { t, lang } = useI18n();
  const months = getMonths(lang);
  const { step, meta, messages, draft, metaRef, giftId: savedGiftId, musicState: savedMusicState, actions: act, wasRestored } = useGuidedState(lang);

  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [streaming, setStreaming] = useState(false);
  const [confirmRestart, setConfirmRestart] = useState(false);
  const gen = useMusicGen();
  const bottomRef = useRef<HTMLDivElement>(null);
  const reviewingRef = useRef<HTMLDivElement>(null);
  const runIdRef = useRef(0);

  // Reattach to a persisted generation after a remount (e.g. back from the
  // gift page): without this the finished MusicCard reverted to the
  // ReviewCard's generate button and the completed song looked lost.
  useEffect(() => {
    if (savedGiftId && savedMusicState) void gen.resume(savedGiftId, savedMusicState);
    // Mount-only: resume replays the snapshot taken at load time.
  }, []);

  // Persist terminal generation statuses so the restore above can bring the
  // card back as ready/error instead of a stale "generating".
  useEffect(() => {
    if (gen.state === "ready" || gen.state === "error") act.setMusicState(gen.state);
  }, [gen.state]);

  const bubbles = derivedBubbles(step, meta, t, months);

  // Auto-scroll
  const lastCount = useRef(0);
  const cnt = bubbles.length + (step === "chat" ? messages.length : 0);
  if (cnt !== lastCount.current) { lastCount.current = cnt; setTimeout(() => bottomRef.current?.scrollIntoView({ behavior: "smooth" }), 50); }
  // Init flow on first mount
  useEffect(() => {
    if (wasRestored) return;
    const id = setTimeout(() => act.go("relationship"), 500);
    return () => clearTimeout(id);
  }, []);
  function handleRelPick(value: string, label: string) {
    act.setMeta({ ...metaRef.current, relationship: value === "__custom__" ? "custom" : value, relationshipLabel: label });
    act.go(value === "pet" ? "scenario" : "name");
  }

  function handleName(name: string) {
    act.setMeta({ ...metaRef.current, name });
    act.go(metaRef.current.relationship === "pet" ? "scenario" : "gender");
  }

  function handleGender(label: string) {
    act.setMeta({ ...metaRef.current, gender: label });
    act.go("birthday");
  }

  function handleBirthday(bday: { month: number; day: number } | null) {
    act.setMeta({ ...metaRef.current, birthday: bday });
    act.go("scenario");
  }

  function handleScenario(value: string, label: string) {
    act.setMeta({ ...metaRef.current, scenario: value, scenarioLabel: label });
    act.go("chat");
    // "__custom__" has no preset angle — don't auto-start a turn; let the
    // user describe the moment themselves in the now-enabled chat bar.
    if (value === "__custom__") return;
    setTimeout(() => startChat(null), 400);
  }


  // ── Chat ────────────────────────────────────────

  const [review, setReview] = useState<string | null>(null);
  // Which post-stream quality stage is running (elevate → review), or null.
  const [stage, setStage] = useState<"elevate" | "review" | null>(null);
  // True when any quality stage fell back to the raw draft for this turn.
  const [draftDegraded, setDraftDegraded] = useState(false);

  /** Scroll the bubble list to the end. The instant (`auto`) variant is used
   * by the per-frame reveal and only fires when already near the bottom, so
   * a user who scrolled up to read isn't yanked back down. */
  function scrollToBottom(behavior: ScrollBehavior) {
    const el = bottomRef.current;
    const scroller = el?.parentElement;
    if (!el || !scroller) return;
    if (behavior === "auto" && scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight > 160) return;
    el.scrollIntoView({ behavior, block: "end" });
  }

  async function startChat(userText: string | null) {
    // Bumped on every new turn and on restart. An in-flight turn whose id no
    // longer matches has been superseded and must not touch state.
    const runId = ++runIdRef.current;

    // The transcript must always open on a user turn or the API rejects it.
    // The pill answers already reach the model via the system prompt, so this
    // opening turn is only a nudge — hence hidden from the bubble list.
    const base: GuidedMessage[] = messages.length
      ? messages
      : [{ role: "user", content: "hi", hidden: true }];
    const msgs: GuidedMessage[] = userText
      ? [...base, { role: "user", content: userText }]
      : base;
    act.setMsg(msgs);
    setStreaming(true); setError(null); setStage(null);
    let gotLyrics = false;

    // ── Typewriter reveal ────────────────────────────────────────────
    // Deltas arrive in bursts; they accumulate in `arrived` while this rAF
    // loop reveals them at a steady cadence — adaptive catch-up when far
    // behind, so a fast stream can't leave the reveal lagging for seconds.
    // Rendering goes through setMsgStreaming (persistence throttled): the old
    // per-token setMsg did a full transcript JSON.stringify + sessionStorage
    // write on every delta — O(n²) and the main source of the stutter.
    let arrived = "";
    let shown = 0;
    let rafId: number | null = null;
    const stopReveal = () => { if (rafId !== null) { cancelAnimationFrame(rafId); rafId = null; } };
    const reveal = () => {
      if (runIdRef.current !== runId) return;
      const backlog = arrived.length - shown;
      if (backlog > 0) {
        shown = Math.min(arrived.length, shown + Math.max(2, Math.ceil(backlog / 6)));
        act.setMsgStreaming([...msgs, { role: "assistant", content: arrived.slice(0, shown) }]);
        scrollToBottom("auto");
      }
      rafId = requestAnimationFrame(reveal);
    };
    rafId = requestAnimationFrame(reveal);

    try {
      const m = metaRef.current;
      const gen = streamChat({ messages: msgs, meta: { lang, name: m.name, relationship: m.relationshipLabel, scenario: scenarioParam(m), gender: m.gender, birthday: birthdayParam(m.birthday) }, photos: [] });
      for await (const e of gen) {
        if (runIdRef.current !== runId) return;
        if (e.type === "Delta") {
          arrived += e.text;
        } else if (e.type === "Elevating" || e.type === "Reviewing") {
          // A post-stream quality stage (full LLM call, tens of seconds)
          // starts here — label the wait. The generation stream is over:
          // stop the reveal loop (its per-frame bottom-scroll would yank the
          // viewport past the indicator) and flush the full text now; Done
          // repeats the same flush, harmlessly.
          stopReveal();
          if (arrived) act.setMsg([...msgs, { role: "assistant", content: arrived }]);
          setStage(e.type === "Elevating" ? "elevate" : "review");
          // The indicator renders above the ReviewCard, which fills the
          // viewport — scroll to the indicator itself or the label is invisible.
          setTimeout(() => reviewingRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" }), 50);
        } else if (e.type === "Done") {
          stopReveal();
          if (arrived) act.setMsg([...msgs, { role: "assistant", content: arrived }]);
          if (e.has_lyrics) {
            // Only a lyrics-producing Done updates the degradation state —
            // a chat-turn Done (no lyrics) must not clear a pending note.
            // Any fallen-back stage shows the same generic note.
            setDraftDegraded((e.degraded?.length ?? 0) > 0);
            gotLyrics = true;
            // Guided turns always send non-null fields (only studio mode
            // emits per-field nulls); the `?? ""` fallbacks are unreachable
            // in practice and exist to satisfy the widened Done type.
            act.setDraft({ lyrics: e.lyrics ?? "", style: e.style ?? "", title: e.title ?? "", vocal: e.vocal || "female" });
            setReview(e.review ?? null);
          }
        } else if (e.type === "Error") {
          stopReveal();
          if (arrived) act.setMsg([...msgs, { role: "assistant", content: arrived }]);
          setError(e.error);
        }
      }
    } catch (e) {
      if (runIdRef.current === runId) setError(e instanceof Error ? e.message : "Chat failed");
    }
    finally {
      stopReveal();
      if (runIdRef.current === runId) {
        setStreaming(false);
        setStage(null);
        if (gotLyrics) act.go("review");
      }
    }
  }

  /** Discard the session and go back to the first question. */
  function handleRestart() {
    runIdRef.current++;
    setConfirmRestart(false);
    setInput(""); setError(null); setReview(null); setStreaming(false); setStage(null); setDraftDegraded(false);
    gen.reset();
    act.reset();
    setTimeout(() => act.go("relationship"), 400);
  }

  async function handleChatSend() {
    const text = input.trim(); if (!text || streaming) return;
    setInput(""); await startChat(text);
  }

  async function handleReviewSubmit(data: ReviewData) {
    const m = metaRef.current;
    setError(null);
    // Start generation first — sets state to "generating" synchronously,
    // so MusicCard never sees "idle"
    const genPromise = gen.start({ lyrics: data.lyrics, style: data.style, title: data.title, vocal: data.vocal, meta: { name: m.name, relationship: m.relationshipLabel, scenario: scenarioParam(m), gender: m.gender, birthday: birthdayParam(m.birthday) }, lang });
    act.go("music");
    const res = await genPromise;
    // Persist the giftId so a remount can restore this card instead of
    // bouncing the user back to the review screen.
    if (res.ok) act.setGift(res.giftId);
  }

  function handleMusicOpen() { if (gen.giftId) { onNavigate(gen.giftId); } }
  function handleMusicRetry() { if (gen.giftId) gen.retry(gen.giftId); }

  // ── Pills configuration per step ─────────────────

  const relPills = RELATIONSHIPS.map((r) => ({ label: t(r.label), value: r.value }));
  const genderPills = [{ label: t("gender_male"), value: "male" }, { label: t("gender_female"), value: "female" }];
  const scenPills = scenarioList(meta.relationship, t);

  // ── Render ──────────────────────────────────────

  const inChat = step === "chat" || step === "review" || step === "music";
  // Nothing worth discarding before the first answer.
  const canRestart = step !== "greet" && step !== "relationship";

  // Progress indicators for the current assistant turn. The chat bubble only
  // shows the text before <<<LYRICS>>>; everything past the marker streams
  // invisibly, so the two phases need their own labels.
  const lastMsg = messages[messages.length - 1];
  const lastAssistant = lastMsg && lastMsg.role === "assistant" ? lastMsg : null;
  const lastTurnText = lastAssistant
    ? stripMarkers(lastAssistant.content.split("<<<LYRICS>>>")[0]).trim()
    : "";
  const lyricsStreaming = streaming && !!lastAssistant && lastAssistant.content.includes("<<<LYRICS>>>");

  return (
    <div className="chat-panel">
      <div className="chat-panel-top">
        <span className="dot-row"><span className={`dot ${!inChat ? "on" : "past"}`} /><span className={`dot ${inChat ? "on" : ""}`} /></span>
        {canRestart && (
          <button className="restart-btn" onClick={() => setConfirmRestart(true)} title={t("restart")} aria-label={t("restart")}>↻</button>
        )}
      </div>
      {confirmRestart && (
        <div className="restart-confirm" role="alertdialog" aria-label={t("restart_q")}>
          <span className="restart-q">{t("restart_q")}</span>
          <button className="restart-yes" onClick={handleRestart}>{t("restart_yes")}</button>
          <button className="restart-no" onClick={() => setConfirmRestart(false)}>{t("restart_cancel")}</button>
        </div>
      )}
      <div className="chat-messages guided">
        {bubbles.map((b, i) => <div key={`b-${i}`} className={`bubble ${b.role}`}>{b.text}</div>)}

        {step === "relationship" && <PillsRow options={relPills} onSelect={handleRelPick} />}
        {step === "relationship" && <GoldPill label={t("instrumental_btn")} onClick={() => { clearGuided(); onSwitchToFree?.(); }} />}

        {step === "name" && <InlineInput placeholder={t("name_placeholder")} onSubmit={handleName} />}

        {step === "gender" && <PillsRow options={genderPills} onSelect={(_, l) => handleGender(l)} />}

        {step === "birthday" && <BirthdayPicker months={months} skipLabel={t("bday_skip")} dayPlaceholder={t("bday_day_placeholder")} onPick={handleBirthday} />}

        {step === "scenario" && <PillsRow options={scenPills} onSelect={handleScenario} />}
        {inChat && messages.map((msg, i) => {
          if (msg.hidden) return null;
          const raw = msg.content;
          // Stop at <<<LYRICS>>> — lyrics belong in ReviewCard, not the chat bubble
          const display = msg.role === "assistant"
            ? stripMarkers(raw.split("<<<LYRICS>>>")[0])
            : stripMarkers(raw);
          if (!display.trim()) return null;
          return <div key={`m-${i}`} className={`bubble ${msg.role === "assistant" ? "bot" : msg.role}`}>{display}</div>;
        })}

        {/* Typing indicator: waiting for the first visible text of the
            current assistant turn. Applies to every turn, not just the
            first — later turns used to show nothing until the first token
            landed. Hidden once the lyrics marker arrives (that phase gets
            its own label below) and while a quality stage is labeled. */}
        {streaming && !stage && !lyricsStreaming && !lastTurnText && (
          <div className="bubble bot typing-indicator" aria-label="Assistant is typing">
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="typing-dot" />
          </div>
        )}

        {/* Lyrics payload streaming: the chat text is done and the hidden
            <<<LYRICS>>> block (the bulk of the tokens on the final turn) is
            arriving. Previously this whole phase was dead air. */}
        {streaming && !stage && lyricsStreaming && (
          <div className="bubble bot reviewing-indicator" role="status">
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="reviewing-label">{t("writing_lyrics")}</span>
          </div>
        )}

        {/* Post-stream quality stages (elevate → review): full LLM calls run
            after the stream ends. Label the wait — previously this was silent
            dead air with the input greyed out. Shown on any step: follow-up
            edits from the review screen regenerate lyrics and hit the same
            wait. */}
        {stage && (
          <div ref={reviewingRef} className="bubble bot reviewing-indicator" role="status">
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="reviewing-label">{t(stage === "elevate" ? "elevating" : "reviewing")}</span>
          </div>
        )}

        {/* Pre-generation transition: gen.start() fired but no giftId yet —
            animate it like the other waiting phases, not a static bubble. */}
        {step === "music" && !gen.giftId && gen.state === "generating" && (
          <div className="bubble bot reviewing-indicator" role="status">
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="reviewing-label">{t("creating_gift")}</span>
          </div>
        )}

        {step === "review" && draft && <ReviewCard key={draft.lyrics} lyrics={draft.lyrics} style={draft.style} title={draft.title} vocal={draft.vocal} styleTags={getStyleTags(lang)} onSubmit={handleReviewSubmit} creating={gen.state === "generating"} review={review ?? undefined} degraded={draftDegraded} />}
        {step === "music" && gen.giftId && <MusicCard initialState={gen.state === "ready" ? "ready" : gen.state === "error" ? "error" : "generating"} onOpen={handleMusicOpen} onRetry={handleMusicRetry} />}

        <div ref={bottomRef} />
      </div>
      {(error || gen.error) && <div className="error-msg" style={{ margin: "0 16px 8px" }}>{error || gen.error}</div>}
      <div className="chat-bar">
        <textarea value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !isImeComposing(e)) { e.preventDefault(); handleChatSend(); } }} rows={1} disabled={streaming || !(step === "chat" || step === "review")} />
        <button className="chat-send-btn" onClick={handleChatSend} disabled={streaming} aria-label="Send"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M2 21l21-9L2 3v7l15 2-15 2z" /></svg></button>
      </div>
    </div>
  );
}
