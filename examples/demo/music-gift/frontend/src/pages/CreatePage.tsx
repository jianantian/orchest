import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useI18n } from "../i18n";
import { GuidedFlow } from "../components/GuidedFlow";
import { FreeCreatePanel } from "../components/FreeCreatePanel";

type CreateTab = "guided" | "free";

export default function CreatePage() {
  const navigate = useNavigate();
  const { t, lang } = useI18n();
  const [tab, setTab] = useState<CreateTab>("guided");

  useEffect(() => {
    function onSwitch(e: Event) {
      const detail = (e as CustomEvent<string>).detail;
      if (detail === "free") setTab("free");
    }
    window.addEventListener("switch-tab", onSwitch);
    return () => window.removeEventListener("switch-tab", onSwitch);
  }, []);

  function handleNavigate(giftId: string) {
    navigate(`/gift/${giftId}`);
  }

  return (
    <div className="create-page">
      <div className="tab-bar">
        <button
          className={`tab-btn ${tab === "guided" ? "active" : ""}`}
          onClick={() => setTab("guided")}
        >
          {t("tab_guided")}
        </button>
        <button
          className={`tab-btn ${tab === "free" ? "active" : ""}`}
          onClick={() => setTab("free")}
        >
          {t("tab_free")}
        </button>
      </div>

      {tab === "guided" ? (
        <GuidedFlow onNavigate={handleNavigate} />
      ) : (
        <FreeCreatePanel lang={lang} photos={[]} onNavigate={handleNavigate} />
      )}
    </div>
  );
}
