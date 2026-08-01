import { useEffect, useState } from "react";
import { useAuth } from "../hooks/useAuth";
import { useI18n } from "../i18n";

type View = "login" | "forgot";

export function LoginModal() {
  const { refresh } = useAuth();
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<View>("login");
  const [isRegister, setIsRegister] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [sent, setSent] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    function onOpen() { setOpen(true); }
    window.addEventListener("open-login", onOpen);
    return () => window.removeEventListener("open-login", onOpen);
  }, []);

  async function handlePasswordSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!email.includes("@")) return;
    setLoading(true);
    setError(null);

    const endpoint = isRegister ? "/api/auth/register" : "/api/auth/login";
    const body: Record<string, string> = { email, password };
    if (isRegister) body.display_name = displayName || email.split("@")[0];

    try {
      const res = await fetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (!res.ok) {
        setError(data.error === "EMAIL_EXISTS" ? t("login_err_exists") :
                 data.error === "INVALID_CREDENTIALS" ? t("login_err_creds") :
                 data.error === "WEAK_PASSWORD" ? t("login_err_weak") :
                 data.error || t("login_err_generic"));
        return;
      }
      await refresh();
      setOpen(false);
    } catch {
      setError(t("login_err_network"));
    } finally {
      setLoading(false);
    }
  }

  async function handleForgotSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!email.includes("@")) return;
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/auth/forgot", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email }),
      });
      if (!res.ok) throw new Error();
      setSent(true);
    } catch {
      setError(t("login_err_send"));
    } finally {
      setLoading(false);
    }
  }

  function handleGoogle() {
    window.location.href = "/api/auth/oauth/google";
  }

  if (!open) return null;

  return (
    <div className="login-overlay" onClick={() => setOpen(false)}>
      <div className="login-modal" onClick={(e) => e.stopPropagation()}>
        {view === "forgot" ? (
          <>
            <h2 className="login-title">{t("forgot_title")}</h2>
            <p className="login-hint">{t("forgot_sub")}</p>
            <form onSubmit={handleForgotSubmit}>
              <input
                className="login-input"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="your@email.com"
                required
                disabled={loading}
              />
              {error && <p className="login-error">{error}</p>}
              {sent && <p className="login-hint">{t("forgot_sent")}</p>}
              <button className="login-btn" type="submit" disabled={loading || !email.includes("@")}>
                {loading ? t("login_wait") : t("forgot_send")}
              </button>
            </form>
            <button
              className="login-link-text"
              disabled={loading}
              onClick={() => {
                setView("login");
                setSent(false);
                setError(null);
              }}
            >
              {t("forgot_back")}
            </button>
          </>
        ) : (
          <>
            <h2 className="login-title">{t("sign_in")}</h2>

            <button className="login-btn oauth-btn" onClick={handleGoogle}>
              <svg width="18" height="18" viewBox="0 0 24 24">
                <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 01-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"/>
                <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
                <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
                <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
              </svg>
              {t("login_google")}
            </button>

            <div className="login-divider"><span>{t("login_or")}</span></div>

            <form onSubmit={handlePasswordSubmit}>
              <input
                className="login-input"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="your@email.com"
                required
                disabled={loading}
              />
              {isRegister && (
                <input
                  className="login-input"
                  type="text"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  placeholder={t("login_name_ph")}
                  required
                  disabled={loading}
                />
              )}
              <input
                className="login-input"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={t("login_password")}
                required
                minLength={8}
                disabled={loading}
              />
              {error && <p className="login-error">{error}</p>}
              <button className="login-btn" type="submit" disabled={loading || !email.includes("@")}>
                {loading ? t("login_wait") : isRegister ? t("login_create") : t("sign_in")}
              </button>
            </form>

            <p className="login-toggle">
              {isRegister ? t("login_have") : t("login_nothave")}{" "}
              <button className="login-link-text" onClick={() => { setIsRegister(!isRegister); setError(null); }}>
                {isRegister ? t("sign_in") : t("login_create_one")}
              </button>
            </p>

            {!isRegister && (
              <button
                className="login-link-text login-forgot"
                onClick={() => {
                  setView("forgot");
                  setSent(false);
                  setError(null);
                }}
              >
                {t("login_forgot")}
              </button>
            )}
          </>
        )}
        <button className="login-close" onClick={() => setOpen(false)}>{t("close")}</button>
      </div>
    </div>
  );
}
