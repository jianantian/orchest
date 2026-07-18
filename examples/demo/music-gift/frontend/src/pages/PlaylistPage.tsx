import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import type { PlaylistItem } from '../types';
import { getPlaylist, setGiftPublished } from '../api';
import AudioPlayer from '../components/AudioPlayer';
import { creatorToken } from '../lib/creator';
import { useI18n } from '../i18n';

export default function PlaylistPage() {
  const { t } = useI18n();
  const [items, setItems] = useState<PlaylistItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [unlisting, setUnlisting] = useState<string | null>(null);

  useEffect(() => {
    getPlaylist()
      .then((res) => setItems(res.items))
      .catch((e) => setError(e instanceof Error ? e.message : 'Failed to load playlist'))
      .finally(() => setLoading(false));
  }, []);

  /** Taking a song off this list is unlisting, not deleting — the share link
   *  keeps working. Permanent delete lives on the gift's own page. */
  async function handleUnlist(id: string) {
    const token = creatorToken(id);
    if (!token) return;
    setUnlisting(id);
    setError(null);
    try {
      await setGiftPublished(id, token, false);
      setItems((prev) => prev.filter((i) => i.id !== id));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not unlist');
    } finally {
      setUnlisting(null);
    }
  }

  if (loading) {
    return (
      <div className="playlist-page loading-page">
        <span className="spinner" /> Loading playlist…
      </div>
    );
  }

  if (error) {
    return <div className="playlist-page"><div className="error-msg">{error}</div></div>;
  }

  return (
    <div className="playlist-page">
      <h1 className="page-title">Playlist</h1>
      <p className="page-sub">Songs crafted with love</p>

      {items.length === 0 ? (
        <div className="empty-state">
          <p>No songs yet.</p>
          <Link to="/" className="btn btn-primary">Create one</Link>
        </div>
      ) : (
        <div className="playlist-grid">
          {items.map((item) => (
            <div key={item.id} className="card playlist-card">
              {creatorToken(item.id) !== null && (
                <button
                  className="playlist-unlist"
                  title={t('unlist')}
                  aria-label={t('unlist')}
                  disabled={unlisting === item.id}
                  onClick={() => void handleUnlist(item.id)}
                >
                  {unlisting === item.id ? <span className="spinner" /> : '×'}
                </button>
              )}
              <Link to={`/gift/${item.id}`} className="playlist-card-link">
                <h3 className="playlist-card-title">{item.title || 'Untitled'}</h3>
                <p className="playlist-card-meta">
                  {item.name}{item.relationship && ` · ${item.relationship}`}
                </p>
                {item.style && <span className="playlist-card-style">♪ {item.style}</span>}
              </Link>
              {item.audio_url && (
                <AudioPlayer src={item.audio_url} compact />
              )}
              <div className="playlist-card-footer">
                <span>♥ {item.likes}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
