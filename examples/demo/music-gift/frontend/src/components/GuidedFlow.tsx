import { useEffect, useRef, useState } from "react";
import type { ChatMessage } from "../types";
import { streamChat } from "../api";
import { useI18n, getMonths } from "../i18n";
import { useMusicGen } from "../hooks/useMusicGen";
import { useGuidedState, clearGuided, type FlowStep } from "../hooks/useGuidedState";
import { DEFAULT_STYLE_TAGS } from "../lib/styles";
import { ReviewCard, type ReviewData } from "./ReviewCard";
import { MusicCard } from "./MusicCard";
import { PillsRow, GoldPill, InlineInput, BirthdayPicker } from "./ChatUI";

const RELATIONSHIPS = [
  { label: "rel_kid", value: "kid" }, { label: "rel_partner", value: "partner" },
  { label: "rel_friend", value: "friend" }, { label: "rel_parent", value: "parent" },
  { label: "rel_pet", value: "pet" }, { label: "rel_custom", value: "__custom__" },
];

function scenarioList(rel: string, t: (k: string) => string): Array<{ label: string; value: string }> {
  const base: Record<string, string[]> = {
    kid: ["Birthday", "Graduation", "Daily Life", "Encouragement"],
    partner: ["Anniversary", "Date Night", "Apology", "Just Because"],
    friend: ["Birthday", "Thank You", "Inside Joke", "Travel Memory"],
    parent: ["Birthday", "Mother's Day", "Father's Day", "Gratitude"],
    pet: ["Gotcha Day", "Daily Joy", "Memorial"],
    custom: ["Birthday", "Anniversary", "Thank You"],
  };
  const labels = base[rel] ?? base.custom;
  return [...labels.map((l) => ({ label: l, value: l.toLowerCase().replace(/\s+/g, "") })), { label: t("scenario_custom"), value: "__custom__" }];
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
  push("bot", t("name_q"));

  if (step === "name") return b;

  push("user", meta.name);

  if (meta.relationship !== "pet") {
    push("bot", t("gender_q"));
    if (step === "gender") return b;
    push("user", meta.gender);
  }

  push("bot", t("bday_q"));
  if (step === "birthday") return b;
  push("user", meta.birthday ? `${months[meta.birthday.month - 1]} ${meta.birthday.day}` : t("bday_skip"));
  push("bot", t("scenario_q"));
  if (step === "scenario") return b;
  push("user", meta.scenarioLabel);

  return b;
}

// ── Component ────────────────────────────────────────────

export function GuidedFlow({ onNavigate, onSwitchToFree }: { onNavigate: (giftId: string) => void; onSwitchToFree?: () => void }) {
  const { t, lang } = useI18n();
  const months = getMonths(lang);
  const { step, meta, messages, metaRef, actions: act, wasRestored } = useGuidedState(lang);

  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [streaming, setStreaming] = useState(false);
  const gen = useMusicGen();
  const bottomRef = useRef<HTMLDivElement>(null);

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
    if (value === "__custom__") return;
    act.setMeta({ ...metaRef.current, scenario: value, scenarioLabel: label });
    act.go("chat");
    setTimeout(() => startChat(null), 400);
  }


  // ── Chat ────────────────────────────────────────

  const [review, setReview] = useState<string | null>(null);

  async function startChat(userText: string | null) {
    const msgs: ChatMessage[] = userText
      ? [...messages, { role: "user" as const, content: userText }]
      : [{ role: "user" as const, content: "hi" }];
    if (userText) act.setMsg(msgs);
    setStreaming(true); setError(null); setReview(null);
    let full = "";
    let done = false;

    try {
      const m = metaRef.current;
      const gen = streamChat({ messages: msgs, meta: { lang, name: m.name, relationship: m.relationshipLabel, scenario: m.scenarioLabel, gender: m.gender }, photos: [] });
      for await (const e of gen) {
        if (e.type === "Delta") {
          full += e.text;
          act.setMsg(userText ? [...msgs, { role: "assistant", content: full }] : [{ role: "assistant", content: full }]);
        } else if (e.type === "Done") {
          done = true;
          if (e.has_lyrics) {
            // Done event carries the reviewed lyrics from the review pass
            act.setMsg([{ role: "assistant", content: full || "" }]);
            setReview(e.review ?? null);
          }
        } else if (e.type === "Error") setError(e.error);
      }
    } catch (e) { setError(e instanceof Error ? e.message : "Chat failed"); }
    finally {
      setStreaming(false);
      if (done && full.includes("<<<LYRICS>>>")) act.go("review");
    }
  }

  async function handleChatSend() {
    const text = input.trim(); if (!text || streaming) return;
    setInput(""); await startChat(text);
  }

  // ── Extracted from the last assistant message ──
  const lastMsg = messages.filter((m) => m.role === "assistant").pop();
  const lyrics = lastMsg?.content?.match(/<<<LYRICS>>>([\s\S]*?)<<<END>>>/)?.[1]?.trim() ?? "";
  const style = lastMsg?.content?.match(/<<<STYLE>>>([\s\S]*?)<<<STYLE_END>>>/)?.[1]?.trim() ?? "";
  const title = lastMsg?.content?.match(/<<<TITLE>>>([\s\S]*?)<<<TITLE_END>>>/)?.[1]?.trim() ?? "";
  const vocal = lastMsg?.content?.match(/<<<VOCAL>>>([\s\S]*?)<<<VOCAL_END>>>/)?.[1]?.trim() ?? "female";

  async function handleReviewSubmit(data: ReviewData) {
    const m = metaRef.current;
    setError(null);
    // Start generation first — sets state to "generating" synchronously,
    // so MusicCard never sees "idle"
    const genPromise = gen.start({ lyrics: data.lyrics, style: data.style, title: data.title, vocal: data.vocal, meta: { name: m.name, relationship: m.relationshipLabel, scenario: m.scenarioLabel, gender: m.gender }, lang });
    act.go("music");
    await genPromise;
    if (gen.error) setError(gen.error);
  }

  function handleMusicOpen() { if (gen.giftId) { clearGuided(); onNavigate(gen.giftId); } }
  function handleMusicRetry() { if (gen.giftId) gen.retry(gen.giftId); }

  // ── Pills configuration per step ─────────────────

  const relPills = RELATIONSHIPS.map((r) => ({ label: t(r.label), value: r.value }));
  const genderPills = [{ label: t("gender_male"), value: "male" }, { label: t("gender_female"), value: "female" }];
  const scenPills = scenarioList(meta.relationship, t);

  // ── Render ──────────────────────────────────────

  const inChat = step === "chat" || step === "review" || step === "music";

  return (
    <div className="chat-panel">
      <span className="dot-row"><span className={`dot ${!inChat ? "on" : "past"}`} /><span className={`dot ${inChat ? "on" : ""}`} /></span>
      <div className="chat-messages guided">
        {bubbles.map((b, i) => <div key={`b-${i}`} className={`bubble ${b.role}`}>{b.text}</div>)}

        {step === "relationship" && <PillsRow options={relPills} onSelect={handleRelPick} />}
        {step === "relationship" && <GoldPill label={t("instrumental_btn")} onClick={() => onSwitchToFree?.()} />}

        {step === "name" && <InlineInput placeholder={t("name_placeholder")} onSubmit={handleName} />}

        {step === "gender" && <PillsRow options={genderPills} onSelect={(_, l) => handleGender(l)} />}

        {step === "birthday" && <BirthdayPicker months={months} skipLabel={t("bday_skip")} onPick={handleBirthday} />}

        {step === "scenario" && <PillsRow options={scenPills} onSelect={handleScenario} />}
        {step === "chat" && messages.map((msg, i) => <div key={`m-${i}`} className={`bubble ${msg.role === "assistant" ? "bot" : msg.role}`}>{msg.content.replace(/<<<[^>]+>>>/g, "")}</div>)}

        {/* Typing indicator: shown while waiting for the first assistant response */}
        {streaming && step === "chat" && messages.filter(m => m.role === "assistant").length === 0 && (
          <div className="bubble bot typing-indicator" aria-label="Assistant is typing">
            <span className="typing-dot" />
            <span className="typing-dot" />
            <span className="typing-dot" />
          </div>
        )}

        {/* Pre-generation transition: gen.start() fired but no giftId yet */}
        {step === "music" && !gen.giftId && gen.state === "generating" && (
          <div className="bubble bot" style={{ opacity: 0.7 }}>Creating your gift, one moment...</div>
        )}

        {step === "review" && lyrics && <ReviewCard lyrics={lyrics} style={style} title={title} vocal={vocal} styleTags={DEFAULT_STYLE_TAGS} onSubmit={handleReviewSubmit} creating={gen.state === "generating"} review={review ?? undefined} />}
        {step === "music" && gen.giftId && <MusicCard initialState={gen.state === "ready" ? "ready" : gen.state === "error" ? "error" : "generating"} onOpen={handleMusicOpen} onRetry={handleMusicRetry} />}

        <div ref={bottomRef} />
      </div>
      {error && <div className="error-msg" style={{ margin: "0 16px 8px" }}>{error}</div>}
      <div className="chat-bar">
        <textarea value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleChatSend(); } }} rows={1} disabled={streaming || !(step === "chat" || step === "review")} />
        <button className="chat-send-btn" onClick={handleChatSend} disabled={streaming} aria-label="Send"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M2 21l21-9L2 3v7l15 2-15 2z" /></svg></button>
      </div>
    </div>
  );
}
