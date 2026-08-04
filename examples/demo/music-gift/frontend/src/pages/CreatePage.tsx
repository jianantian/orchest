import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useI18n } from "../i18n";
import { GuidedFlow } from "../components/GuidedFlow";
import { Studio } from "../components/Studio";

type CreateTab = "guided" | "free";

export default function CreatePage() {
  const navigate = useNavigate();
  const { t, lang } = useI18n();
  const [tab, setTab] = useState<CreateTab>("guided");

  function handleNavigate(giftId: string) {
    navigate(`/gift/${giftId}`);
  }

  function handleSwitchToFree() {
    setTab("free");
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
        <GuidedFlow onNavigate={handleNavigate} onSwitchToFree={handleSwitchToFree} />
      ) : (
        <Studio lang={lang} photos={[]} onNavigate={handleNavigate} />
      )}
    </div>
  );
}
