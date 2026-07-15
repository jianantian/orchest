import { useRef, useState } from "react";
import type { ChatMessage } from "../types";

export type FlowStep = "greet" | "relationship" | "name" | "gender" | "birthday" | "scenario" | "chat" | "review" | "music";

export interface StepMeta {
  relationship: string; relationshipLabel: string;
  name: string; gender: string;
  birthday: { month: number; day: number } | null;
  scenario: string; scenarioLabel: string;
}

interface Saved {
  lang: string; step: FlowStep; meta: StepMeta; messages: ChatMessage[];
}

const KEY = "moment_guided";

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
  const restored = load();
  const snapshot = restored?.lang === lang ? restored : null;

  const [step, setStep] = useState<FlowStep>(snapshot?.step ?? "greet");
  const [meta, rawSetMeta] = useState<StepMeta>(snapshot?.meta ?? {
    relationship: "", relationshipLabel: "", name: "", gender: "",
    birthday: null, scenario: "", scenarioLabel: "",
  });
  const [messages, setMessages] = useState<ChatMessage[]>(snapshot?.messages ?? []);

  const metaRef = useRef(meta);
  metaRef.current = meta;

  function persist(s: FlowStep) {
    save({ lang, step: s, meta: metaRef.current, messages: messages.slice(-20) });
  }

  function go(s: FlowStep) { setStep(s); persist(s); }
  function setMeta(m: StepMeta) { rawSetMeta(m); metaRef.current = m; persist(step); }
  function setMsg(ms: ChatMessage[]) { setMessages(ms); save({ lang, step, meta: metaRef.current, messages: ms.slice(-20) }); }

  return {
    step, meta, messages, metaRef,
    actions: { go, setMeta, setMsg },
    wasRestored: !!snapshot,
  };
}
