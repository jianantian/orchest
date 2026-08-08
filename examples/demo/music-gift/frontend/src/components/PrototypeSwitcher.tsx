// PROTOTYPE — throwaway, delete after Studio workbench design is approved.
// Floating view switcher for /prototype/studio. Dev-only.
import { useEffect } from "react";
import { useSearchParams } from "react-router-dom";

export interface ProtoView {
  key: string;
  name: string;
}

export function PrototypeSwitcher({ views, current }: { views: ProtoView[]; current: string }) {
  const [, setSearchParams] = useSearchParams();

  const cycle = (dir: 1 | -1) => {
    const idx = views.findIndex((v) => v.key === current);
    const next = views[(idx + dir + views.length) % views.length];
    setSearchParams({ view: next.key }, { replace: true });
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement ||
          (el instanceof HTMLElement && el.isContentEditable)) return;
      if (e.key === "ArrowLeft") cycle(-1);
      if (e.key === "ArrowRight") cycle(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current]);

  const active = views.find((v) => v.key === current) ?? views[0];

  return (
    <div className="proto-switcher" role="group" aria-label="view switcher">
      <button onClick={() => cycle(-1)} aria-label="previous view">←</button>
      <span className="proto-switcher-label"><b>{active.key}</b>{active.name}</span>
      <button onClick={() => cycle(1)} aria-label="next view">→</button>
    </div>
  );
}
