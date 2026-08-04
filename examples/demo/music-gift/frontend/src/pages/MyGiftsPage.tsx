import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import type { Gift } from '../types';
import { claimGifts, deleteGift, getGift, getMyGifts, setGiftPublished } from '../api';
import AudioPlayer from '../components/AudioPlayer';
import {
  allCreatorTokens,
  creatorGiftIds,
  creatorToken,
  forgetCreatorToken,
} from '../lib/creator';
import { useAuth } from '../hooks/useAuth';
import { useI18n } from '../i18n';

type Busy = { id: string; action: 'publish' | 'delete' } | null;

function statusOf(gift: Gift): { key: string; creating: boolean } {
  switch (gift.gen_status) {
    case 'pending':
    case 'running':
      return { key: 'mine_status_creating', creating: true };
    case 'done':
      return { key: 'mine_status_ready', creating: false };
    case 'failed':
      return { key: 'mine_status_failed', creating: false };
    default:
      return { key: 'mine_status_none', creating: false };
  }
}

/**
 * Every gift of the current creator, two sources merged:
 *
 * - **Device tokens** (localStorage): gifts created on this browser, proof
 *   held as creator_token. Anonymous visitors rely on this alone.
 * - **Account** (`GET /api/my-gifts`): gifts whose `creator_id` is the
 *   signed-in user — includes gifts claimed from this device and gifts
 *   created (or claimed) on any other device.
 *
 * When signed in, the device tokens are claimed to the account first
 * (idempotent), so gifts created before login follow the account across
 * devices. Mutations pass the device token when present; the backend also
 * accepts a matching session, which is what makes them work on devices
 * that never saw the token.
 */
export default function MyGiftsPage() {
  const { user, login } = useAuth();
  const { t } = useI18n();
  const [items, setItems] = useState<Gift[]>([]);
  const [loading, setLoading] = useState(true);
  const [claiming, setClaiming] = useState(false);
  /** Fatal: the initial load failed — replaces the page. */
  const [loadError, setLoadError] = useState<string | null>(null);
  /** Non-fatal: an action (publish / delete) failed — shown inline. */
  const [actionError, setActionError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      // Signed in: attach this device's gifts to the account first, so the
      // account fetch below (and every other device) sees them. Failures are
      // silent — tokens stay put and the next visit retries.
      if (user) {
        setClaiming(true);
        try {
          await claimGifts(allCreatorTokens());
        } catch {
          // keep tokens; the next visit retries
        }
        setClaiming(false);
      }

      const deviceIds = creatorGiftIds();
      const results = await Promise.allSettled(deviceIds.map((id) => getGift(id)));
      if (cancelled) return;

      const byId = new Map<string, Gift>();
      for (let i = 0; i < results.length; i++) {
        const result = results[i];
        if (result.status === 'fulfilled') {
          byId.set(deviceIds[i], result.value);
        } else if (
          // Gift vanished server-side (deleted elsewhere): 404 is the only
          // terminal answer. Any other failure (network, 5xx) keeps the token
          // so a later visit retries instead of silently forgetting ownership.
          result.reason instanceof Error && /: 404$/.test(result.reason.message)
        ) {
          forgetCreatorToken(deviceIds[i]);
        }
      }

      if (user) {
        try {
          const account = await getMyGifts();
          for (const gift of account) byId.set(gift.id, gift);
        } catch {
          // Account fetch failed — device gifts still render below.
        }
      }

      const gifts = [...byId.values()].sort(
        (a, b) => Number(b.created_at) - Number(a.created_at),
      );
      setItems(gifts);
      setLoading(false);
    })().catch(() => {
      if (!cancelled) {
        setLoadError(t('mine_err_load'));
        setLoading(false);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [t, user]);

  async function handlePublishToggle(gift: Gift, next: boolean) {
    const token = creatorToken(gift.id) ?? undefined;
    const sessionOwns = user !== null && gift.creator_id === user.id;
    if (!token && !sessionOwns) return;
    setBusy({ id: gift.id, action: 'publish' });
    setActionError(null);
    try {
      await setGiftPublished(gift.id, token, next);
      setItems((prev) =>
        prev.map((g) => (g.id === gift.id ? { ...g, published: next } : g)),
      );
    } catch (e) {
      setActionError(e instanceof Error ? e.message : t('mine_err_publish'));
    } finally {
      setBusy(null);
    }
  }

  async function handleDelete(id: string) {
    const token = creatorToken(id) ?? undefined;
    const sessionOwns = user !== null && items.some((g) => g.id === id && g.creator_id === user.id);
    if (!token && !sessionOwns) return;
    setBusy({ id, action: 'delete' });
    setActionError(null);
    try {
      await deleteGift(id, token);
      forgetCreatorToken(id);
      setItems((prev) => prev.filter((g) => g.id !== id));
      setConfirmId(null);
    } catch (e) {
      setActionError(e instanceof Error ? e.message : t('mine_err_delete'));
      setConfirmId(null);
    } finally {
      setBusy(null);
    }
  }

  if (loading) {
    return (
      <div className="playlist-page loading-page">
        <span className="spinner" /> {t('loading_gift')}
      </div>
    );
  }

  if (loadError) {
    return <div className="playlist-page"><div className="error-msg">{loadError}</div></div>;
  }

  return (
    <div className="playlist-page">
      <h1 className="page-title">{t('mine_title')}</h1>
      <p className="page-sub">{t('mine_sub')}</p>

      <div className="mine-account">
        {user ? (
          <>
            <span>{t('mine_account_hint')}</span>
            <Link to="/set-password" className="mine-btn">{t('setpw_title')}</Link>
          </>
        ) : (
          <span>
            {t('mine_login_hint')}{' '}
            <button className="login-link-text" onClick={login}>{t('sign_in')}</button>
          </span>
        )}
      </div>

      {claiming && (
        <div className="mine-account mine-claiming">
          <span className="spinner" /> {t('mine_claiming')}
        </div>
      )}

      {actionError && <div className="error-msg">{actionError}</div>}

      {items.length === 0 ? (
        <div className="empty-state">
          <p>{t('mine_empty')}</p>
          <Link to="/" className="btn btn-primary">{t('playlist_create')}</Link>
        </div>
      ) : (
        <div className="playlist-grid">
          {items.map((gift) => {
            const status = statusOf(gift);
            const owned =
              creatorToken(gift.id) !== null ||
              (user !== null && gift.creator_id === user.id);
            const cardBusy = busy?.id === gift.id;
            return (
              <div key={gift.id} className="card playlist-card">
                <Link to={`/gift/${gift.id}`} className="playlist-card-link">
                  <h3 className="playlist-card-title">{gift.meta.title || 'Untitled'}</h3>
                  <p className="playlist-card-meta">
                    {gift.meta.name || ''}
                    {gift.meta.relationship && ` · ${gift.meta.relationship}`}
                  </p>
                  {gift.meta.style && <span className="playlist-card-style">♪ {gift.meta.style}</span>}
                </Link>
                {gift.audio_url && gift.gen_status === 'done' && (
                  <AudioPlayer src={gift.audio_url} compact />
                )}
                <div className="playlist-card-footer">
                  <span className={`mine-status${status.creating ? ' creating' : ''}`}>
                    {status.creating && <span className="spinner" />}
                    {t(status.key)}
                    {!gift.published && <span className="mine-private">· {t('mine_private')}</span>}
                  </span>
                  {owned && (
                    <span className="mine-actions">
                      <Link to={`/?edit=${gift.id}`} className="mine-btn">
                        {t('edit')}
                      </Link>
                      <button
                        className="mine-btn"
                        disabled={cardBusy}
                        onClick={() => void handlePublishToggle(gift, !gift.published)}
                      >
                        {cardBusy && busy?.action === 'publish' ? (
                          <span className="spinner" />
                        ) : gift.published ? (
                          t('unlist')
                        ) : (
                          t('publish_label')
                        )}
                      </button>
                      {confirmId === gift.id ? (
                        <span className="owner-confirm">
                          <span className="owner-confirm-q">{t('delete_q')}</span>
                          <button
                            className="owner-delete-yes"
                            disabled={cardBusy}
                            onClick={() => void handleDelete(gift.id)}
                          >
                            {cardBusy && busy?.action === 'delete' ? (
                              <span className="spinner" />
                            ) : (
                              t('delete_yes')
                            )}
                          </button>
                          <button
                            className="owner-delete-no"
                            disabled={cardBusy}
                            onClick={() => setConfirmId(null)}
                          >
                            {t('delete_cancel')}
                          </button>
                        </span>
                      ) : (
                        <button className="mine-btn danger" onClick={() => setConfirmId(gift.id)}>
                          {t('delete_gift')}
                        </button>
                      )}
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
