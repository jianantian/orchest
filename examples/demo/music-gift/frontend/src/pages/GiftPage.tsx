import { useCallback, useEffect, useRef, useState } from 'react';
import { useParams } from 'react-router-dom';
import type { Gift } from '../types';
import { generateMusic, getGift, likeGift, pollGenerateStatus } from '../api';
import AudioPlayer from '../components/AudioPlayer';

export default function GiftPage() {
  const { id } = useParams<{ id: string }>();
  const [gift, setGift] = useState<Gift | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [genStatus, setGenStatus] = useState<string | null>(null);
  const [liked, setLiked] = useState(false);
  const [likeCount, setLikeCount] = useState(0);
  const [copied, setCopied] = useState(false);
  const pollRef = useRef<number | undefined>(undefined);

  const startPolling = useCallback(() => {
    clearInterval(pollRef.current);
    pollRef.current = setInterval(async () => {
      if (!id) return;
      try {
        const status = await pollGenerateStatus(id);
        setGenStatus(status.status);
        if (status.status === 'done') {
          setGenerating(false);
          setGift((g) => (g ? { ...g, audio_url: status.audio_url, gen_status: 'done' } : g));
          clearInterval(pollRef.current);
        } else if (status.status === 'failed') {
          setGenerating(false);
          setError('Music generation failed');
          clearInterval(pollRef.current);
        }
      } catch {
        // Keep polling on transient errors
      }
    }, 3000);
  }, [id]);

  useEffect(() => {
    if (!id) return;
    setLoading(true);
    getGift(id)
      .then((g) => {
        setGift(g);
        setLikeCount(g.likes.length);
        setGenStatus(g.gen_status);
        if (g.gen_status === 'pending' || g.gen_status === 'running') {
          setGenerating(true);
          startPolling();
        }
      })
      .catch((e) => setError(e instanceof Error ? e.message : 'Failed to load gift'))
      .finally(() => setLoading(false));

    return () => {
      clearInterval(pollRef.current);
    };
  }, [id, startPolling]);

  async function handleGenerate() {
    if (!id) return;
    setGenerating(true);
    setError(null);
    setGenStatus('pending');
    try {
      await generateMusic(id);
      startPolling();
    } catch (e) {
      setGenerating(false);
      setGenStatus(null);
      setError(e instanceof Error ? e.message : 'Generation failed');
    }
  }

  async function handleLike() {
    if (!id) return;
    const viewerId = localStorage.getItem('viewer_id') || crypto.randomUUID();
    localStorage.setItem('viewer_id', viewerId);
    try {
      const res = await likeGift(id, viewerId);
      setLiked(res.liked);
      setLikeCount(res.likes);
    } catch {
      // Like is best-effort
    }
  }

  function handleShare() {
    const url = window.location.href;
    navigator.clipboard.writeText(url).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  if (loading) {
    return (
      <div className="gift-page loading-page">
        <span className="spinner" /> Loading gift…
      </div>
    );
  }

  if (error && !gift) {
    return <div className="gift-page"><div className="error-msg">{error}</div></div>;
  }

  if (!gift) return null;

  const title = gift.meta.title ?? 'Untitled';
  const style = gift.meta.style ?? '';
  const name = gift.meta.name ?? '';
  const relationship = gift.meta.relationship ?? '';

  return (
    <div className="gift-page">
      <div className="gift-hero">
        <h1 className="gift-title">{title}</h1>
        {name && (
          <p className="gift-dedication">
            for {name}{relationship && `, ${relationship}`}
          </p>
        )}
        {style && <span className="gift-style-tag">♪ {style}</span>}
      </div>

      {gift.audio_url ? (
        <AudioPlayer src={gift.audio_url} title={title} />
      ) : generating ? (
        <div className="gift-generating">
          <span className="spinner" />
          <p>Generating music…</p>
          <span className="gen-status">{genStatus}</span>
        </div>
      ) : (
        <button className="btn btn-primary btn-lg btn-full" onClick={() => void handleGenerate()}>
          Generate Music
        </button>
      )}

      {error && <div className="error-msg">{error}</div>}

      {gift.lyrics && (
        <div className="card gift-lyrics">
          <h3 className="gift-section-title">Lyrics</h3>
          <pre className="lyrics-body">{gift.lyrics}</pre>
        </div>
      )}

      <div className="gift-actions">
        <button className="btn btn-secondary" onClick={() => void handleLike()}>
          {liked ? '♥' : '♡'} {likeCount}
        </button>
        <button className="btn btn-secondary" onClick={handleShare}>
          {copied ? 'Copied!' : 'Share'}
        </button>
      </div>
    </div>
  );
}
