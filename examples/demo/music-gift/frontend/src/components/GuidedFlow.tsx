import { useCallback, useEffect, useRef, useState } from "react";
import type { ChatMessage, GiftMeta } from "../types";
import { createGift, generateMusic, pollGenerateStatus, streamChat } from "../api";
import { useI18n, getMonths } from "../i18n";
import { ReviewCard, type ReviewData } from "./ReviewCard";
import { MusicCard, type MusicCardState } from "./MusicCard";

interface StepState {
  relationship: string;
  relationshipLabel: string;
  name: string;
  gender: string;
  birthday: { month: number; day: number } | null;
  scenario: string;
  scenarioLabel: string;
}

type FlowStep =
  | "greet"
  | "relationship"
  | "name"
  | "gender"
  | "birthday"
  | "scenario"
  | "chat"
  | "review"
  | "music"
  | "paste";

interface BubbleItem {
  type: "bot" | "user";
  text: string;
}

interface PillsItem {
  type: "pills";
  options: Array<{ label: string; value: string }>;
  onSelect: (value: string, label: string) => void;
}

interface PillItem {
  type: "pill";
  label: string;
  gold?: boolean;
  onSelect: () => void;
}

interface InputItem {
  type: "input";
  placeholder: string;
  onSubmit: (value: string) => void;
}

interface BirthdayItem {
  type: "birthday";
  onPick: (bday: { month: number; day: number } | null) => void;
}

type UIItem = BubbleItem | PillsItem | PillItem | InputItem | BirthdayItem;

const RELATIONSHIPS = [
  { label: "rel_kid", value: "kid" },
  { label: "rel_partner", value: "partner" },
  { label: "rel_friend", value: "friend" },
  { label: "rel_parent", value: "parent" },
  { label: "rel_pet", value: "pet" },
  { label: "rel_custom", value: "__custom__" },
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

export function GuidedFlow({ onNavigate }: { onNavigate: (giftId: string) => void }) {
  const { t, lang } = useI18n();
  const months = getMonths(lang);

  const [step, setStep] = useState<FlowStep>("greet");
  const [meta, setMeta] = useState<StepState>({
    relationship: "", relationshipLabel: "", name: "", gender: "",
    birthday: null, scenario: "", scenarioLabel: "",
  });
  const metaRef = useRef(meta);
  metaRef.current = meta;

  // Chat state
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [bubbles, setBubbles] = useState<BubbleItem[]>([]);
  const [uiItems, setUiItems] = useState<UIItem[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);

  // Lyrics
  const [lyrics, setLyrics] = useState("");
  const [inferredStyle, setInferredStyle] = useState("");
  const [inferredTitle, setInferredTitle] = useState("");
  const [inferredVocal, setInferredVocal] = useState("female");

  // Music card
  const [creating, setCreating] = useState(false);
  const [musicState, setMusicState] = useState<MusicCardState>("generating");
  const [giftId, setGiftId] = useState<string | null>(null);

  const chatRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const initRef = useRef(false);

  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [bubbles, uiItems, messages, scrollToBottom]);

  // Start flow
  useEffect(() => {
    if (initRef.current) return;
    initRef.current = true;

    setTimeout(() => {
      addBot(t("greet"));
      setTimeout(() => {
        addBot(t("relationship_q"));
        showRelPills();
      }, 400);
    }, 200);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function addBot(text: string) {
    setBubbles((prev) => [...prev, { type: "bot", text }]);
  }

  function addUser(text: string) {
    setBubbles((prev) => [...prev, { type: "user", text }]);
  }

  // --- Relationship ---

  function showRelPills() {
    const options = RELATIONSHIPS.map((r) => ({ label: t(r.label), value: r.value }));
    setUiItems([
      {
        type: "pills",
        options,
        onSelect: (value, label) => {
          if (value === "__custom__") {
            // Show custom input
            setUiItems([{ type: "input", placeholder: t("rel_custom_placeholder"), onSubmit: (v) => handleRelPick("custom", v) }]);
          } else {
            handleRelPick(value, label);
          }
        },
      },
    ]);
    // Instrumental shortcut appears after delay
    setTimeout(() => {
      setUiItems((prev) => [
        ...prev,
        { type: "pill", label: t("instrumental_btn"), gold: true, onSelect: () => startInstrumental() },
      ]);
    }, 500);
  }

  function handleRelPick(value: string, label: string) {
    setMeta((prev) => ({ ...prev, relationship: value, relationshipLabel: label }));
    addUser(label);
    setUiItems([]);
    setTimeout(() => {
      addBot(t("name_q"));
      setUiItems([{ type: "input", placeholder: t("name_placeholder"), onSubmit: handleName }]);
    }, 350);
  }

  function handleName(name: string) {
    setMeta((prev) => ({ ...prev, name }));
    addUser(name);
    setUiItems([]);

    if (metaRef.current.relationship === "pet") {
      setTimeout(() => goScenario(), 350);
    } else {
      setTimeout(() => {
        addBot(t("gender_q"));
        setUiItems([
          {
            type: "pills",
            options: [
              { label: t("gender_male"), value: "male" },
              { label: t("gender_female"), value: "female" },
            ],
            onSelect: (_, label) => handleGender(label),
          },
        ]);
      }, 350);
    }
  }

  function handleGender(label: string) {
    setMeta((prev) => ({ ...prev, gender: label }));
    addUser(label);
    setUiItems([]);
    setTimeout(() => {
      addBot(t("bday_q"));
      setUiItems([{ type: "birthday", onPick: handleBirthday }]);
    }, 350);
  }

  function handleBirthday(bday: { month: number; day: number } | null) {
    setMeta((prev) => ({ ...prev, birthday: bday }));
    if (bday) {
      addUser(`${months[bday.month - 1]} ${bday.day}`);
    } else {
      addUser(t("bday_skip"));
    }
    setUiItems([]);
    setTimeout(() => goScenario(), 350);
  }

  function goScenario() {
    addBot(t("scenario_q"));
    const scenarios = scenarioList(metaRef.current.relationship, t);
    setUiItems([
      {
        type: "pills",
        options: scenarios,
        onSelect: (value, label) => {
          if (value === "__custom__") {
            setUiItems([{ type: "input", placeholder: t("scenario_custom_placeholder"), onSubmit: (v) => handleScenario(v, v) }]);
          } else {
            handleScenario(value, label);
          }
        },
      },
    ]);
  }

  function handleScenario(value: string, label: string) {
    setMeta((prev) => ({ ...prev, scenario: value, scenarioLabel: label }));
    addUser(label);
    setUiItems([]);
    setStep("chat");
    // Start the LLM call
    setTimeout(() => startChat(null), 350);
  }

  // --- Free chat ---

  async function startChat(userText: string | null) {
    const isAutoStart = userText === null;
    const userMsg: ChatMessage = { role: "user", content: userText || "hi" };
    const chatMsgs: ChatMessage[] = isAutoStart
      ? [userMsg]
      : [...messages, userMsg];

    // Only show typed user messages, not the auto-generated "hi"
    if (!isAutoStart) {
      setMessages((prev) => [...prev, userMsg]);
    }
    setStreaming(true);
    setError(null);

    let assistantText = "";

    try {
      const m = metaRef.current;
      const giftMeta: GiftMeta = {
        lang,
        name: m.name,
        relationship: m.relationshipLabel,
        scenario: m.scenarioLabel,
        gender: m.gender,
      };

      const gen = streamChat({ messages: chatMsgs, meta: giftMeta, photos: [] });

      for await (const event of gen) {
        if (event.type === "Delta") {
          assistantText += event.text;
          if (isAutoStart) {
            setMessages([{ role: "assistant", content: assistantText }]);
          } else {
            setMessages([...chatMsgs, { role: "assistant", content: assistantText }]);
          }

          const hasLyrics = assistantText.includes("<<<LYRICS>>>");
          if (hasLyrics) {
            const lyricsMatch = assistantText.match(/<<<LYRICS>>>([\s\S]*?)<<<END>>>/);
            const styleMatch = assistantText.match(/<<<STYLE>>>([\s\S]*?)<<<STYLE_END>>>/);
            const titleMatch = assistantText.match(/<<<TITLE>>>([\s\S]*?)<<<TITLE_END>>>/);
            const vocalMatch = assistantText.match(/<<<VOCAL>>>([\s\S]*?)<<<VOCAL_END>>>/);

            if (lyricsMatch) {
              setLyrics(lyricsMatch[1].trim());
              setInferredStyle(styleMatch?.[1]?.trim() ?? "");
              setInferredTitle(titleMatch?.[1]?.trim() ?? "");
              setInferredVocal(vocalMatch?.[1]?.trim() ?? "female");
              setStreaming(false);
              setStep("review");
              addBot(t("lyrics_ready"));
              return;
            }
          }
        } else if (event.type === "Done" || event.type === "Error") {
          if (event.type === "Error") setError(event.error);
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Chat failed");
    } finally {
      setStreaming(false);
    }
  }

  async function handleChatSend() {
    const text = input.trim();
    if (!text || streaming) return;
    setInput("");
    await startChat(text);
  }

  // --- Review ---

  async function handleReviewSubmit(data: ReviewData) {
    setCreating(true);
    setError(null);
    setStep("music");
    setMusicState("generating");

    try {
      const m = metaRef.current;
      const giftMeta: GiftMeta = {
        lang,
        name: m.name,
        relationship: m.relationshipLabel,
        scenario: m.scenarioLabel,
        gender: m.gender,
        title: data.title,
        vocal: data.vocal,
      };

      const res = await createGift({
        lyrics: data.lyrics,
        kind: "song",
        meta: giftMeta,
        photos: [],
        style: data.style,
      });

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

  async function handleMusicRetry() {
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

  // --- Instrumental ---

  function startInstrumental() {
    setBubbles([]);
    setUiItems([]);
    // Delegate to free create for simplicity
    // In reference, this opens an instrumental wizard
    // For now, switch to free tab
    window.dispatchEvent(new CustomEvent("switch-tab", { detail: "free" }));
  }


  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleChatSend();
    }
  }

  // --- Render ---

  return (
    <div className="chat-panel">
      {/* Dot indicators */}
      <span className="dot-row">
        <span className={`dot ${step === "greet" || step === "relationship" || step === "name" || step === "gender" || step === "birthday" || step === "scenario" ? "on" : ""} ${["chat", "review", "music"].includes(step) ? "past" : ""}`} />
        <span className={`dot ${["chat", "review", "music"].includes(step) ? "on" : ""}`} />
      </span>

      {/* Chat area */}
      <div className="chat-messages guided" ref={chatRef}>
        {/* Structured bubbles */}
        {bubbles.map((b, i) => (
          <div key={`b-${i}`} className={`bubble ${b.type}`}>
            {b.text}
          </div>
        ))}

        {/* Structured UI items */}
        {uiItems.map((item, i) => {
          if (item.type === "pills") {
            return <Pills key={`p-${i}`} options={item.options} onSelect={item.onSelect} />;
          }
          if (item.type === "pill") {
            return <Pill key={`pl-${i}`} label={item.label} gold={item.gold} onSelect={item.onSelect} />;
          }
          if (item.type === "input") {
            return <InlineInput key={`in-${i}`} placeholder={item.placeholder} onSubmit={item.onSubmit} />;
          }
          if (item.type === "birthday") {
            return <BirthdayPicker key={`bd-${i}`} months={months} skipLabel={t("bday_skip")} onPick={item.onPick} />;
          }
          return null;
        })}

        {/* Free chat messages */}
        {step === "chat" &&
          messages.map((msg, i) => (
            <div key={`msg-${i}`} className={`bubble ${msg.role === "assistant" ? "bot" : msg.role}`}>
              {msg.content.replace(/<<<[^>]+>>>/g, "")}
            </div>
          ))}

        {/* Review card */}
        {step === "review" && (
          <ReviewCard
            lyrics={lyrics}
            style={inferredStyle}
            title={inferredTitle}
            vocal={inferredVocal}
            styleTags={["pop", "rock", "jazz", "electronic", "folk", "r&b", "romantic", "upbeat", "melancholic"]}
            onSubmit={handleReviewSubmit}
            creating={creating}
          />
        )}

        {/* Music card */}
        {step === "music" && giftId && (
          <MusicCard initialState={musicState} onOpen={handleMusicOpen} onRetry={handleMusicRetry} />
        )}

        <div ref={bottomRef} />
      </div>

      {error && <div className="error-msg" style={{ margin: "0 16px 8px" }}>{error}</div>}

      {/* Chat input bar — always visible */}
      <div className="chat-bar">
        <textarea
          ref={inputRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          rows={1}
          disabled={streaming}
        />
        <button
          className="chat-send-btn"
          onClick={handleChatSend}
          disabled={streaming}
          aria-label="Send"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <path d="M2 21l21-9L2 3v7l15 2-15 2z" />
          </svg>
        </button>
      </div>

    </div>
  );
}

// ── Sub-components ────────────────────────────────────

function Pills({ options, onSelect }: { options: Array<{ label: string; value: string }>; onSelect: (v: string, l: string) => void }) {
  const [selected, setSelected] = useState<string | null>(null);

  return (
    <div className="pills-wrap">
      {options.map((o) => (
        <span
          key={o.value}
          className={`pill ${selected === o.value ? "on" : ""}`}
          onClick={() => {
            setSelected(o.value);
            onSelect(o.value, o.label);
          }}
          role="button"
          tabIndex={0}
        >
          {o.label}
        </span>
      ))}
    </div>
  );
}

function Pill({ label, gold, onSelect }: { label: string; gold?: boolean; onSelect: () => void }) {
  return (
    <div className="pills-wrap">
      <span
        className="pill"
        style={gold ? { color: "var(--gold)", borderColor: "var(--gold)" } : undefined}
        onClick={onSelect}
        role="button"
        tabIndex={0}
      >
        {label}
      </span>
    </div>
  );
}

function InlineInput({ placeholder, onSubmit }: { placeholder: string; onSubmit: (v: string) => void }) {
  const [value, setValue] = useState("");

  function done() {
    const v = value.trim();
    if (!v) return;
    onSubmit(v);
  }

  return (
    <div className="inline-input">
      <input
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") done();
        }}
        placeholder={placeholder}
        maxLength={30}
        autoFocus
      />
      <button onClick={done}>OK</button>
    </div>
  );
}


function daysInMonth(month: number): number {
  if (month === 2) return 28;
  if ([4, 6, 9, 11].includes(month)) return 30;
  return 31;
}
function BirthdayPicker({
  months,
  skipLabel,
  onPick,
}: {
  months: string[];
  skipLabel: string;
  onPick: (bday: { month: number; day: number } | null) => void;
}) {
  const [selectedMonth, setSelectedMonth] = useState<number | null>(null);
  const [day, setDay] = useState("");

  function selectMonth(idx: number) {
    setSelectedMonth(idx);
  }

  function submitDay() {
    const d = parseInt(day, 10);
    if (d >= 1 && d <= daysInMonth(selectedMonth ?? 1) && selectedMonth !== null) {
      onPick({ month: selectedMonth + 1, day: d });
    }
  }


  return (
    <>
      <div className="bday-grid">
        {months.map((m, i) => (
          <button key={m} className={selectedMonth === i ? "on" : ""} onClick={() => selectMonth(i)}>
            {m}
          </button>
        ))}
      </div>
      {selectedMonth !== null && (
        <div className="inline-input" style={{ marginTop: 8 }}>
          <input
            type="number"
            value={day}
            onChange={(e) => {
              const raw = e.target.value;
              // Allow empty input while typing
              if (raw === "") { setDay(""); return; }
              let v = parseInt(raw, 10);
              if (isNaN(v)) return;
              // Wrap: below 1 → last day of month, above max → 1
              const maxDay = daysInMonth(selectedMonth + 1);
              if (v < 1) v = maxDay;
              else if (v > maxDay) v = 1;
              setDay(String(v));
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") submitDay();
            }}
            placeholder="Day"
            style={{ width: 80 }}
            autoFocus
          />
          <button onClick={submitDay}>OK</button>
        </div>
      )}
      <div style={{ alignSelf: "flex-start", marginTop: 6 }}>
        <button className="btn-small" onClick={() => onPick(null)}>
          {skipLabel}
        </button>
      </div>
    </>
  );
}
