import { useCallback, useEffect, useRef, useState } from "react";
import { useParams, Link } from "react-router-dom";
import type { Gift } from "../types";
import { generateMusic, getGift, likeGift } from "../api";
import AudioPlayer from "../components/AudioPlayer";
import { LRCViewer, type LRCLine } from "../components/LRCViewer";
import { parseLRC } from "../lib/lrc";
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
  const [revealed, setRevealed] = useState(false);
  const [lrcLines, setLrcLines] = useState<LRCLine[] | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
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
        if (g.lrc) setLrcLines(parseLRC(g.lrc));
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load gift"))
      .finally(() => setLoading(false));
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
      {showUnwrap && <UnwrapStage title={title} name={name} onClose={() => setRevealed(true)} />}
      <div className={`gift-page${revealed ? " revealed" : ""}`}>
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
          <AudioPlayer src={gift.audio_url} title={title} onTimeUpdate={setCurrentTime} />
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

        {lrcLines && lrcLines.length > 0 ? (
          <div className="gift-lyrics-card">
            <LRCViewer lines={lrcLines} currentTime={currentTime} onSeek={(t) => {
              const audio = document.querySelector("audio");
              if (audio) audio.currentTime = t;
            }} />
          </div>
        ) : gift.lyrics ? (
          <div className="gift-lyrics-card">
            {renderLyrics(gift.lyrics)}
          </div>
        ) : null}

        <div className="gift-actions">
          <Link to="/" className="btn btn-secondary">Edit & Try Again</Link>
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

/** Render lyrics matching reference: parse [Section] markers, <br> between lines. */
function renderLyrics(raw: string) {
  // Strip <<<MARKER>>> tags
  let text = raw.replace(/<<<[A-Z_]+>>>[^<]*<<<[A-Z_]+>>>/g, "").replace(/<<<[A-Z_]+>>>/g, "").trim();
  const sections = parseLyrics(text);
  if (!sections.length) return <div className="lyric-lines">{text}</div>;

  return sections.map((s, i) => (
    <div key={i} className="lyric-section">
      <div className="lyric-label">{s.label}</div>
      <div className="lyric-lines">
        {s.lines.map((line, j) => (
          <span key={j}>
            {j > 0 && <br />}
            {line}
          </span>
        ))}
      </div>
    </div>
  ));
}

function parseLyrics(raw: string): Array<{ label: string; lines: string[] }> {
  const parts = raw.split(/\[([^\]]+)\]/).filter(Boolean);
  const sections: Array<{ label: string; content: string }> = [];
  for (let i = 0; i < parts.length; i += 2) {
    const label = parts[i].trim();
    const content = parts[i + 1]?.trim() || "";
    if (content) sections.push({ label, content });
  }
  // Merge consecutive choruses
  const merged: Array<{ label: string; content: string }> = [];
  for (const s of sections) {
    const last = merged[merged.length - 1];
    if (last && last.label === s.label && last.label.toLowerCase().includes("chorus")) {
      last.content += "\n\n" + s.content;
    } else {
      merged.push({ ...s });
    }
  }
  return merged.map((s) => ({ label: s.label, lines: s.content.split("\n").map((l) => l.trim()).filter(Boolean) }));
}
