import { useEffect, useRef, useState } from 'react';

interface AudioPlayerProps {
  src: string;
  title?: string;
  compact?: boolean;
}

export default function AudioPlayer({ src, title, compact }: AudioPlayerProps) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [current, setCurrent] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const el = audioRef.current;
    if (!el) return;
    const audio = el; // non-null alias for closures

    function onLoaded() {
      setDuration(audio.duration || 0);
      setLoading(false);
    }
    function onTime() {
      setCurrent(audio.currentTime);
    }
    function onEnd() {
      setPlaying(false);
      setCurrent(0);
    }

    audio.addEventListener('loadedmetadata', onLoaded);
    audio.addEventListener('timeupdate', onTime);
    audio.addEventListener('ended', onEnd);
    audio.addEventListener('canplay', onLoaded);

    return () => {
      audio.removeEventListener('loadedmetadata', onLoaded);
      audio.removeEventListener('timeupdate', onTime);
      audio.removeEventListener('ended', onEnd);
      audio.removeEventListener('canplay', onLoaded);
    };
  }, [src]);

  function toggle() {
    const el = audioRef.current;
    if (!el) return;
    if (playing) {
      el.pause();
      setPlaying(false);
    } else {
      void el.play();
      setPlaying(true);
    }
  }

  function seek(e: React.MouseEvent<HTMLDivElement>) {
    const el = audioRef.current;
    if (!el || !duration) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = (e.clientX - rect.left) / rect.width;
    el.currentTime = pct * duration;
    setCurrent(el.currentTime);
  }

  const pct = duration > 0 ? (current / duration) * 100 : 0;

  function fmt(t: number): string {
    if (!t || isNaN(t)) return '0:00';
    const m = Math.floor(t / 60);
    const s = Math.floor(t % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  return (
    <div className={compact ? 'audio-player compact' : 'audio-player'}>
      <audio ref={audioRef} src={src} preload="metadata" />
      <button className="audio-play-btn" onClick={toggle} disabled={loading} aria-label={playing ? 'Pause' : 'Play'}>
        {loading ? (
          <span className="spinner" />
        ) : playing ? (
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <rect x="6" y="4" width="4" height="16" rx="1" />
            <rect x="14" y="4" width="4" height="16" rx="1" />
          </svg>
        ) : (
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
            <path d="M8 5v14l11-7z" />
          </svg>
        )}
      </button>
      <div className="audio-info">
        {title && <div className="audio-title">{title}</div>}
        <div className="audio-progress-row">
          <span className="audio-time">{fmt(current)}</span>
          <div className="audio-progress" onClick={seek}>
            <div className="audio-progress-fill" style={{ width: `${pct}%` }} />
          </div>
          <span className="audio-time">{fmt(duration)}</span>
        </div>
      </div>
    </div>
  );
}
