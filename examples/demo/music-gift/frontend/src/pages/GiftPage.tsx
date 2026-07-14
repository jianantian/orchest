import { useCallback, useEffect, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import type { Gift } from "../types";
import { generateMusic, getGift, likeGift } from "../api";
import AudioPlayer from "../components/AudioPlayer";
import { UnwrapStage, shouldShowUnwrap } from "../components/UnwrapStage";

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
  const [countdownHtml, setCountdownHtml] = useState<string | null>(null);
  const [countdownPending, setCountdownPending] = useState(false);
  const esRef = useRef<EventSource | null>(null);
  const cdPollRef = useRef<number | undefined>(undefined);

  const startPolling = useCallback(() => {
    esRef.current?.close();
    if (!id) return;
    const es = new EventSource(`/api/generate/${id}/stream`);
    esRef.current = es;
    es.onmessage = (e) => {
      try {
        const data = JSON.parse(e.data);
        setGenStatus(data.status);
        if (data.status === "done") {
          setGenerating(false);
          setGift((g) => (g ? { ...g, audio_url: data.audio_url, gen_status: "done" } : g));
          es.close();
        } else if (data.status === "failed" || data.status === "timeout" || data.status === "error") {
          setGenerating(false);
          if (data.status === "failed") setError("Music generation failed");
          else if (data.status === "timeout") setError("Generation timed out. Try again.");
          es.close();
        }
      } catch { /* ignore parse errors */ }
    };
    es.onerror = () => { es.close(); setGenerating(false); };
  }, [id]);

  useEffect(() => {
    if (!id) return;
    setLoading(true);
    getGift(id)
      .then((g) => {
        setGift(g);
        setLikeCount(g.likes.length);
        setGenStatus(g.gen_status);
        if (g.gen_status === "pending" || g.gen_status === "running") {
          setGenerating(true);
          startPolling();
        }
        // Load countdown section if gift has one
        loadCountdown(g.id, g.countdown_status);
      })

    return () => { esRef.current?.close(); };
  }, [id, startPolling]);

  async function handleGenerate() {
    if (!id) return;
    setGenerating(true);
    setError(null);
    setGenStatus("pending");
    try {
      await generateMusic(id);
      startPolling();
    } catch (e) {
      setGenerating(false);
      setGenStatus(null);
      setError(e instanceof Error ? e.message : "Generation failed");
    }
  }

  async function handleLike() {
    if (!id) return;
    const viewerId = localStorage.getItem("viewer_id") || crypto.randomUUID();
    localStorage.setItem("viewer_id", viewerId);
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

  async function loadCountdown(giftId: string, cdStatus?: string | null) {
    if (cdStatus === "ready") {
      try {
        const res = await fetch(`/api/countdown-section/${giftId}`);
        if (res.ok) setCountdownHtml(await res.text());
      } catch {
        // Countdown is optional
      }
    } else if (cdStatus === "pending") {
      setCountdownPending(true);
      cdPollRef.current = setInterval(async () => {
        try {
          const res = await fetch(`/api/countdown-section/${giftId}`);
          if (res.ok) {
            clearInterval(cdPollRef.current);
            setCountdownPending(false);
            setCountdownHtml(await res.text());
          }
        } catch {
          // Keep polling
        }
      }, 5000);
    }
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

  const title = gift.meta.title ?? "Untitled";
  const style = gift.meta.style ?? "";
  const name = gift.meta.name ?? "";
  const relationship = gift.meta.relationship ?? "";
  const showUnwrap = id ? shouldShowUnwrap(id) : false;

  return (
    <>
      {showUnwrap && <UnwrapStage title={title} name={name} />}
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


        {/* Countdown section — LLM-generated interactive scene */}
        {countdownPending && (
          <div className="countdown-placeholder">
            <span className="cd-spinner" />
            <span>Creating a special scene for {name || "you"}…</span>
          </div>
        )}
        {countdownHtml && (
          <div
            className="countdown-section"
            dangerouslySetInnerHTML={{ __html: countdownHtml }}
          />
        )}

        {gift.audio_url ? (
          <AudioPlayer src={gift.audio_url} title={title} />
        ) : generating ? (
          <div className="gift-generating">
            <div className="gen-label">
              Creating your song
              <span className="gen-dots">
                <span /><span /><span />
              </span>
            </div>
            <div className="gen-bar-wrap">
              <div className="gen-bar-fill" />
            </div>
            <div className="gen-meta">
              <span>{genStatus || "preparing…"}</span>
              <span>This may take a minute</span>
            </div>
          </div>
        ) : (
          <button className="btn btn-primary btn-lg btn-full" onClick={() => void handleGenerate()}>
            Generate Music
          </button>
        )}

        {error && <div className="error-msg">{error}</div>}

        {gift.lyrics && (
          <div className="gift-lyrics-card">
            <h3 className="gift-section-title">Lyrics</h3>
            <div className="lyric-lines">{renderLyrics(gift.lyrics)}</div>
          </div>
        )}

        <div className="gift-actions">
          <button className="btn btn-secondary" onClick={() => void handleLike()}>
            {liked ? "♥" : "♡"} {likeCount}
          </button>
          <button className="btn btn-secondary" onClick={handleShare}>
            {copied ? "Copied!" : "Share"}
          </button>
        </div>
      </div>
    </>
  );
}

/** Render lyrics with [verse]/[chorus] section labels in reference style. */
function renderLyrics(text: string) {
  const lines = text.split("\n");
  const elements: React.ReactNode[] = [];
  let currentSection: string | null = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    const match = line.match(/^\[([^\]]+)\]/i);
    if (match) {
      currentSection = match[1];
      elements.push(
        <div key={`s-${i}`} className="lyric-section-label">
          {currentSection}
        </div>,
      );
    } else if (line) {
      elements.push(
        <p key={i} className="lyric-line">{line}</p>,
      );
    } else {
      elements.push(<br key={i} />);
    }
  }

  return elements;
}
