import { useEffect, useRef, useState } from "react";
import { useI18n } from "../i18n";

interface UnwrapStageProps {
  title: string;
  name: string;
  onClose?: () => void;
}


const MOTE_COUNT = 12;

type Phase = "idle" | "opening" | "closing";

export function UnwrapStage({ title, name, onClose }: UnwrapStageProps) {
  const { t } = useI18n();
  // Drive the whole lifecycle through React state. The previous version called
  // stage.remove() to tear itself down imperatively — but this node is
  // React-owned, so when the parent then re-rendered and unmounted <UnwrapStage>
  // React's removeChild hit a node that was already gone, threw, and blanked
  // the entire app on a recipient's first screen. Never touch the node
  // directly; let React add and remove it.
  const [phase, setPhase] = useState<Phase>("idle");
  const stageRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef(false);
  const posRef = useRef({ x: 0, y: 0 });
  const timers = useRef<number[]>([]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    // Lock background scroll while the overlay is up (the CSS existed but
    // nothing ever added the class, so this never actually worked).
    document.body.classList.add("locked");

    // Spawn floating motes
    const motes: HTMLDivElement[] = [];
    for (let i = 0; i < MOTE_COUNT; i++) {
      const m = document.createElement("div");
      m.className = "mote";
      const size = 2 + Math.random() * 4;
      m.style.width = `${size}px`;
      m.style.height = `${size}px`;
      m.style.left = `${Math.random() * 100}%`;
      m.style.setProperty("--mx", `${(Math.random() - 0.5) * 200}px`);
      const dur = 16 + Math.random() * 14;
      m.style.animationDuration = `${dur}s`;
      m.style.animationDelay = `${-Math.random() * dur}s`;
      stage.appendChild(m);
      motes.push(m);
    }

    // Parallax on mouse move
    const handleMove = (e: MouseEvent) => {
      const rect = stage.getBoundingClientRect();
      posRef.current.x = ((e.clientX - rect.left) / rect.width - 0.5) * 2;
      posRef.current.y = ((e.clientY - rect.top) / rect.height - 0.5) * 2;
      if (!rafRef.current) {
        rafRef.current = true;
        requestAnimationFrame(() => {
          rafRef.current = false;
          if (sceneRef.current) {
            sceneRef.current.style.transform = `rotateX(${-18 - posRef.current.y * 3}deg) rotateY(${-26 + posRef.current.x * 7}deg)`;
          }
        });
      }
    };

    const handleLeave = () => {
      if (sceneRef.current) sceneRef.current.style.transform = "";
    };

    stage.addEventListener("mousemove", handleMove, { passive: true });
    stage.addEventListener("mouseleave", handleLeave);

    return () => {
      document.body.classList.remove("locked");
      stage.removeEventListener("mousemove", handleMove);
      stage.removeEventListener("mouseleave", handleLeave);
      motes.forEach((m) => m.remove());
      timers.current.forEach(clearTimeout);
    };
  }, []);

  function handleOpen() {
    if (phase !== "idle") return;
    setPhase("opening");

    timers.current.push(
      window.setTimeout(() => setPhase("closing"), 2000),
      window.setTimeout(() => {
        // Persist the flag BEFORE onClose so the parent's re-render recomputes
        // shouldShowUnwrap() as false and unmounts us cleanly — React removes
        // the node, we never do.
        try {
          const giftId = location.pathname.split("/").pop();
          if (giftId) sessionStorage.setItem(`moment_unwrapped_${giftId}`, "1");
        } catch {
          // ignore
        }
        onClose?.();
      }, 3500),
    );
  }

  const stageClass = phase === "closing" ? "opening closing" : phase === "opening" ? "opening" : "";

  return (
    <div ref={stageRef} id="unwrap-stage" className={stageClass} role="button" aria-label="Open your gift" tabIndex={0} onClick={handleOpen}>
      <div id="unwrap-titles">
        <div id="unwrap-eyebrow">{t("unwrap_eyebrow")}</div>
        <div id="unwrap-headline">
          {title || name || t("unwrap_fallback")}
        </div>
        <div className="ornament">
          <span className="line" />
          <span className="dia" />
          <span className="line" />
        </div>
      </div>

      {/* Simplified 3D box */}
      <div id="box-scene" ref={sceneRef}>
        <div id="box-base">
          <div className="face cream front">
            <span className="wordmark">Moment</span>
          </div>
          <div className="face cream back" />
          <div className="face cream shadowed left" />
          <div className="face cream shadowed right" />
          <div className="face cream shadowed bottom" />
          <div className="face velvet interior" />
        </div>
        <div id="box-lid">
          <div className="face cream front" />
          <div className="face cream back" />
          <div className="face cream shadowed left" />
          <div className="face cream shadowed right" />
          <div className="face cream lit-top top">
            <div className="lid-mono">
              <span className="mono-letter">M</span>
            </div>
          </div>
          <div className="face bottom" />
        </div>
        <div id="light-burst" />
      </div>

      <div id="box-shadow" />

      <div id="tap-hint">
        <span>{t("unwrap_hint")}</span>
        <span className="arrow" />
      </div>
    </div>
  );
}

/** Check whether to show the unwrap stage for this gift. */
export function shouldShowUnwrap(giftId: string): boolean {
  try {
    return !sessionStorage.getItem(`moment_unwrapped_${giftId}`);
  } catch {
    return true;
  }
}
