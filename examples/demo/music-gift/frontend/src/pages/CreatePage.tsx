import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useI18n } from "../i18n";
import { GuidedFlow } from "../components/GuidedFlow";
import { Studio } from "../components/Studio";

type CreateTab = "guided" | "free";

const TAB_FADE_MS = 150;

/** Workbench top bar: mode tabs on the left, global actions (draft state,
 * studio-only undo slot) on the right. Edit mode hides the tabs. */
function WorkbenchTopbar({
  active,
  showTabs,
  onSelect,
  undoSlot,
}: {
  active: CreateTab;
  showTabs: boolean;
  onSelect: (tab: CreateTab) => void;
  undoSlot?: ReactNode;
}) {
  const { t } = useI18n();
  return (
    <div className="wb-topbar">
      {showTabs ? (
        <div className="wb-tabs">
          <button
            className={`wb-tab${active === "guided" ? " on" : ""}`}
            onClick={() => onSelect("guided")}
          >
            {t("tab_guided")}
          </button>
          <button
            className={`wb-tab${active === "free" ? " on" : ""}`}
            onClick={() => onSelect("free")}
          >
            {t("tab_free")}
          </button>
        </div>
      ) : (
        <span />
      )}
      <div className="wb-topbar-actions">
        {/* Draft-state text is wired in Task 3/5. */}
        <span className="wb-draft-state" />
        {/* Undo button is injected by Task 3; studio tab only. */}
        {active === "free" ? undoSlot : null}
      </div>
    </div>
  );
}

export default function CreatePage() {
  const navigate = useNavigate();
  const { lang } = useI18n();
  const [searchParams] = useSearchParams();
  /** /?edit={id} opens the studio in edit mode for an existing gift. */
  const editGiftId = searchParams.get("edit");
  const [tab, setTab] = useState<CreateTab>(editGiftId ? "free" : "guided");
  const [fading, setFading] = useState(false);
  const fadeTimer = useRef<number | null>(null);
  /** Target of an in-flight fade, if any. While the fade timer runs `tab`
   * still holds the OLD tab, so comparing a click against `tab` alone
   * would swallow rapid clicks (click A→B, then B→A before the timer fires
   * looked like A→A). Always compare against the effective target. */
  const pendingTab = useRef<CreateTab | null>(null);

  useEffect(
    () => () => {
      if (fadeTimer.current !== null) window.clearTimeout(fadeTimer.current);
    },
    [],
  );

  function handleNavigate(giftId: string) {
    navigate(`/gift/${giftId}`);
  }

  /** Cross-fade tab content: fade out, swap, fade back in. Under reduced
   * motion the swap is immediate with no transition. */
  function switchTab(next: CreateTab) {
    if (next === (pendingTab.current ?? tab)) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      if (fadeTimer.current !== null) window.clearTimeout(fadeTimer.current);
      pendingTab.current = null;
      setFading(false);
      setTab(next);
      return;
    }
    if (fadeTimer.current !== null) window.clearTimeout(fadeTimer.current);
    pendingTab.current = next;
    setFading(true);
    fadeTimer.current = window.setTimeout(() => {
      pendingTab.current = null;
      setTab(next);
      setFading(false);
    }, TAB_FADE_MS);
  }

  // Edit mode forces the studio tab — switching back to guided would drop
  // the editing context, so the tab bar is hidden (showTabs=false).
  return (
    <div className="create-page wb-bench">
      <WorkbenchTopbar active={tab} showTabs={!editGiftId} onSelect={switchTab} />
      <div className={`wb-body${fading ? " fading" : ""}`}>
        {tab === "guided" && !editGiftId ? (
          <GuidedFlow onNavigate={handleNavigate} onSwitchToFree={() => switchTab("free")} />
        ) : (
          <Studio
            lang={lang}
            photos={[]}
            onNavigate={handleNavigate}
            editGiftId={editGiftId ?? undefined}
          />
        )}
      </div>
    </div>
  );
}
