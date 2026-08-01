import { useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { resetPassword } from '../api';
import { useAuth } from '../hooks/useAuth';
import { useI18n } from '../i18n';

/**
 * Landing page of the password-reset email: redeem the one-time token with
 * a new password. A successful reset invalidates old sessions and signs the
 * user in, so the done state links straight to My Gifts.
 */
export default function ResetPasswordPage() {
  const [params] = useSearchParams();
  const token = params.get('token') ?? '';
  const { refresh } = useAuth();
  const { t } = useI18n();
  const [password, setPasswordValue] = useState('');
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [invalid, setInvalid] = useState(false);
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
      await resetPassword(token, password);
      await refresh();
      setDone(true);
    } catch {
      setInvalid(true);
    } finally {
      setBusy(false);
    }
  }

  if (!token) {
    return (
      <div className="playlist-page">
        <h1 className="page-title">{t('reset_title')}</h1>
        <div className="empty-state">
          <p>{t('reset_need_token')}</p>
          <Link to="/" className="btn btn-primary">{t('forgot_back')}</Link>
        </div>
      </div>
    );
  }

  return (
    <div className="playlist-page">
      <h1 className="page-title">{t('reset_title')}</h1>

      {done ? (
        <div className="empty-state">
          <p>{t('reset_done')}</p>
          <Link to="/mine" className="btn btn-primary">{t('mine_title')}</Link>
        </div>
      ) : invalid ? (
        <div className="empty-state">
          <p>{t('reset_invalid')}</p>
          <Link to="/" className="btn btn-primary">{t('forgot_back')}</Link>
        </div>
      ) : (
        <>
          <p className="page-sub">{t('reset_sub')}</p>
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
              {busy ? <span className="spinner" /> : t('reset_submit')}
            </button>
          </form>
        </>
      )}
    </div>
  );
}
