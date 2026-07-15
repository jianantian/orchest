import { useCallback, useEffect, useRef, useState } from "react";
import type { ChatMessage, GiftMeta } from "../types";
import { streamChat } from "../api";
import { useI18n, getMonths } from "../i18n";
import { useMusicGen } from "../hooks/useMusicGen";
import { DEFAULT_STYLE_TAGS } from "../lib/styles";
import { ReviewCard, type ReviewData } from "./ReviewCard";
import { MusicCard } from "./MusicCard";
import { PillsRow, GoldPill, InlineInput, BirthdayPicker } from "./ChatUI";

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

export function GuidedFlow({ onNavigate, onSwitchToFree }: { onNavigate: (giftId: string) => void; onSwitchToFree?: () => void }) {
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

  // Music generation hook
  const gen = useMusicGen();

  // Lyrics state
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

  // --- Review → Music Gen ---

  async function handleReviewSubmit(data: ReviewData) {
    setError(null);
    setStep("music");

    const m = metaRef.current;
    await gen.start({
      lyrics: data.lyrics,
      style: data.style,
      title: data.title,
      vocal: data.vocal,
      meta: {
        name: m.name,
        relationship: m.relationshipLabel,
        scenario: m.scenarioLabel,
        gender: m.gender,
      },
      lang,
    });
    if (gen.error) setError(gen.error);
  }

  function handleMusicOpen() {
    if (gen.giftId) onNavigate(gen.giftId);
  }

  function handleMusicRetry() {
    if (gen.giftId) gen.retry(gen.giftId);
  }

  // --- Instrumental ---
  function startInstrumental() {
    setBubbles([]);
    setUiItems([]);
    onSwitchToFree?.();
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
            return <PillsRow key={`p-${i}`} options={item.options} onSelect={item.onSelect} />;
          }
          if (item.type === "pill") {
            return <GoldPill key={`pl-${i}`} label={item.label} onClick={item.onSelect} />;
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
            styleTags={DEFAULT_STYLE_TAGS}
            onSubmit={handleReviewSubmit}
            creating={gen.state === "generating"}
          />
        )}
        {/* Music card */}
        {step === "music" && gen.giftId && (
          <MusicCard initialState={gen.state === "ready" ? "ready" : gen.state === "error" ? "error" : "generating"} onOpen={handleMusicOpen} onRetry={handleMusicRetry} />
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
