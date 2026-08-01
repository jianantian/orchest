import { useState } from 'react';
import { Link } from 'react-router-dom';
import { setPassword } from '../api';
import { useAuth } from '../hooks/useAuth';
import { useI18n } from '../i18n';

/**
 * Set a new password for the signed-in account. This is the second half of
 * the recovery flow: forgot password → magic-link login → this page. It is
 * also where a passwordless (magic-link-only) user can add password login.
 */
export default function SetPasswordPage() {
  const { user, login } = useAuth();
  const { t } = useI18n();
  const [password, setPasswordValue] = useState('');
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (password.length < 8) {
      setError(t('setpw_err_weak'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await setPassword(password);
      setDone(true);
    } catch {
      setError(t('setpw_err'));
    } finally {
      setBusy(false);
    }
  }

  if (!user) {
    return (
      <div className="playlist-page">
        <h1 className="page-title">{t('setpw_title')}</h1>
        <div className="empty-state">
          <p>{t('setpw_need_login')}</p>
          <button className="btn btn-primary" onClick={login}>{t('sign_in')}</button>
        </div>
      </div>
    );
  }

  return (
    <div className="playlist-page">
      <h1 className="page-title">{t('setpw_title')}</h1>
      <p className="page-sub">{t('setpw_sub')}</p>

      {done ? (
        <div className="empty-state">
          <p>{t('setpw_done')}</p>
          <Link to="/mine" className="btn btn-primary">{t('mine_title')}</Link>
        </div>
      ) : (
        <form className="setpw-form" onSubmit={handleSubmit}>
          <input
            className="login-input"
            type="password"
            value={password}
            onChange={(e) => setPasswordValue(e.target.value)}
            placeholder={t('setpw_ph')}
            required
            minLength={8}
            disabled={busy}
          />
          {error && <p className="login-error">{error}</p>}
          <button className="btn btn-primary" type="submit" disabled={busy || password.length < 8}>
            {busy ? <span className="spinner" /> : t('setpw_submit')}
          </button>
        </form>
      )}
    </div>
  );
}
