import { NavLink, Route, Routes } from "react-router-dom";
import { useI18n, LANGS, LANG_LABELS, type Lang } from "./i18n";
import { AuthProvider, useAuth } from "./hooks/useAuth";
import { LoginModal } from "./components/LoginModal";
import CreatePage from "./pages/CreatePage";
import GiftPage from "./pages/GiftPage";
import PlaylistPage from "./pages/PlaylistPage";

export default function App() {
  return (
    <AuthProvider>
      <AppContent />
      <LoginModal />
    </AuthProvider>
  );
}

function AppContent() {
  const { t, lang, setLang } = useI18n();
  const { user, loading, login, logout } = useAuth();

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
        <div className="header-right">
          <select
            className="lang-switch"
            value={lang}
            onChange={(e) => setLang(e.target.value as Lang)}
          >
            {LANGS.map((l) => (
              <option key={l} value={l}>{LANG_LABELS[l]}</option>
            ))}
          </select>
          {!loading && (
            user ? (
              <button className="user-btn" onClick={logout} title={user.display_name}>
                {user.display_name[0].toUpperCase()}
              </button>
            ) : (
              <button className="login-link" onClick={login}>{t("sign_in")}</button>
            )
          )}
        </div>
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
