import { useState } from "react";
import { isImeComposing } from "../lib/ime";

// ── PillsRow ────────────────────────────────────────────

export function PillsRow({
  options,
  onSelect,
}: {
  options: Array<{ label: string; value: string }>;
  onSelect: (value: string, label: string) => void;
}) {
  const [selected, setSelected] = useState<string | null>(null);

  return (
    <div className="pills-wrap">
      {options.map((o) => (
        <span
          key={o.value}
          className={`opt-pill ${selected === o.value ? "on" : ""}`}
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

// ── GoldPill (instrumental shortcut) ────────────────────

export function GoldPill({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <div className="pills-wrap">
      <span
        className="opt-pill"
        style={{ color: "var(--gold)", borderColor: "var(--gold)" }}
        onClick={onClick}
        role="button"
        tabIndex={0}
      >
        {label}
      </span>
    </div>
  );
}

// ── InlineInput ─────────────────────────────────────────

export function InlineInput({
  placeholder,
  onSubmit,
}: {
  placeholder: string;
  onSubmit: (value: string) => void;
}) {
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
          if (e.key === "Enter" && !isImeComposing(e)) done();
        }}
        placeholder={placeholder}
        maxLength={30}
        autoFocus
      />
      <button onClick={done}>OK</button>
    </div>
  );
}

// ── BirthdayPicker ──────────────────────────────────────

export function BirthdayPicker({
  months,
  skipLabel,
  dayPlaceholder,
  onPick,
}: {
  months: string[];
  skipLabel: string;
  dayPlaceholder: string;
  onPick: (bday: { month: number; day: number } | null) => void;
}) {
  const [selectedMonth, setSelectedMonth] = useState<number | null>(null);
  const [day, setDay] = useState("");

  function submitDay() {
    if (selectedMonth === null) return;
    const d = parseInt(day, 10);
    if (d >= 1 && d <= daysInMonth(selectedMonth + 1)) {
      onPick({ month: selectedMonth + 1, day: d });
    }
  }

  return (
    <>
      <div className="bday-grid">
        {months.map((m, i) => (
          <button key={m} className={selectedMonth === i ? "on" : ""} onClick={() => setSelectedMonth(i)}>
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
              if (raw === "") { setDay(""); return; }
              const v = parseInt(raw, 10);
              if (isNaN(v)) return;
              setDay(String(Math.min(Math.max(v, 1), daysInMonth(selectedMonth + 1))));
            }}
            onKeyDown={(e) => { if (e.key === "Enter") submitDay(); }}
            placeholder={dayPlaceholder}
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

/** `month` is 1-indexed. Feb allows 29 — a birthday has no year to check against. */
function daysInMonth(month: number): number {
  if (month === 2) return 29;
  if ([4, 6, 9, 11].includes(month)) return 30;
  return 31;
}
