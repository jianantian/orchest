import { useEffect } from "react";
import { NavLink, Route, Routes, useLocation } from "react-router-dom";
import { useI18n, LANGS, LANG_LABELS, type Lang } from "./i18n";
import { AuthProvider, useAuth } from "./hooks/useAuth";
import { LoginModal } from "./components/LoginModal";
import { UserMenu } from "./components/UserMenu";
import { ThemeToggle } from "./components/ThemeToggle";
import CreatePage from "./pages/CreatePage";
import GiftPage from "./pages/GiftPage";
import MyGiftsPage from "./pages/MyGiftsPage";
import PlaylistPage from "./pages/PlaylistPage";
import ResetPasswordPage from "./pages/ResetPasswordPage";
import SetPasswordPage from "./pages/SetPasswordPage";

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
  const { pathname } = useLocation();
  useEffect(() => {
    const root = document.getElementById("root");
    root?.classList.toggle("frame-home", pathname === "/");
    return () => root?.classList.remove("frame-home");
  }, [pathname]);

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
          <NavLink to="/mine" className={({ isActive }) => (isActive ? "active" : "")}>
            {t("nav_mine")}
          </NavLink>
        </nav>
        <div className="header-right">
          <ThemeToggle />
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
              <UserMenu user={user} onLogout={logout} />
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
          <Route path="/mine" element={<MyGiftsPage />} />
          <Route path="/set-password" element={<SetPasswordPage />} />
          <Route path="/reset-password" element={<ResetPasswordPage />} />
        </Routes>
      </main>
    </>
  );
}
