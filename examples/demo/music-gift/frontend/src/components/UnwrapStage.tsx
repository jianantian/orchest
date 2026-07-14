import { useEffect, useRef, useState } from "react";

interface UnwrapStageProps {
  title: string;
  name: string;
}

const MOTE_COUNT = 12;

export function UnwrapStage({ title, name }: UnwrapStageProps) {
  const [closing, setClosing] = useState(false);
  const sceneRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef(false);
  const posRef = useRef({ x: 0, y: 0 });

  useEffect(() => {
    // Spawn floating motes
    const motes: HTMLDivElement[] = [];
    const stage = document.getElementById("unwrap-stage");
    if (!stage) return;

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
      stage.removeEventListener("mousemove", handleMove);
      stage.removeEventListener("mouseleave", handleLeave);
    };
  }, []);

  function handleOpen() {
    if (closing) return;
    setClosing(true);
    const stage = document.getElementById("unwrap-stage");
    if (!stage) return;

    document.body.classList.remove("locked");
    stage.classList.add("opening");

    setTimeout(() => stage.classList.add("closing"), 2000);
    setTimeout(() => {
      stage.remove();
      try {
        const giftId = location.pathname.split("/").pop();
        if (giftId) sessionStorage.setItem(`moment_unwrapped_${giftId}`, "1");
      } catch {
        // ignore
      }
    }, 3500);
  }


  return (
    <div id="unwrap-stage" role="button" aria-label="Open your gift" tabIndex={0} onClick={handleOpen}>
      <div id="unwrap-titles">
        <div id="unwrap-eyebrow">A Moment, for you</div>
        <div id="unwrap-headline">
          {title || name || "Something special"}
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
        <span>tap to unwrap</span>
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
