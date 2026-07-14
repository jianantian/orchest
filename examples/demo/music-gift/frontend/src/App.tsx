import { NavLink, Route, Routes } from "react-router-dom";
import { useI18n, LANGS, LANG_LABELS, type Lang } from "./i18n";
import CreatePage from "./pages/CreatePage";
import GiftPage from "./pages/GiftPage";
import PlaylistPage from "./pages/PlaylistPage";

export default function App() {
  const { t, lang, setLang } = useI18n();

  return (
    <>
      <header className="app-header">
        <NavLink to="/" className="app-logo">
          {t("logo")}
        </NavLink>
        <nav className="app-nav">
          <NavLink to="/" end className={({ isActive }) => (isActive ? "active" : "")}>
            {t("nav_create")}
          </NavLink>
          <NavLink to="/playlist" className={({ isActive }) => (isActive ? "active" : "")}>
            {t("nav_playlist")}
          </NavLink>
        </nav>
        <select
          className="lang-switch"
          value={lang}
          onChange={(e) => setLang(e.target.value as Lang)}
        >
          {LANGS.map((l) => (
            <option key={l} value={l}>
              {LANG_LABELS[l]}
            </option>
          ))}
        </select>
      </header>
      <main className="app-main">
        <Routes>
          <Route path="/" element={<CreatePage />} />
          <Route path="/gift/:id" element={<GiftPage />} />
          <Route path="/playlist" element={<PlaylistPage />} />
        </Routes>
      </main>
    </>
  );
}
