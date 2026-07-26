import { useCallback, useEffect, useRef, useState } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import type { Gift } from "../types";
import { generateMusic, getGift, likeGift, watchGeneration, type GenerationWatch } from "../api";
import AudioPlayer from "../components/AudioPlayer";
import { LRCViewer } from "../components/LRCViewer";
import { parseLRC, type LRCLine } from "../lib/lrc";
import { parseLyrics, stripMarkers } from "../lib/styles";
import { UnwrapStage, shouldShowUnwrap } from "../components/UnwrapStage";
import { CountdownFrame } from "../components/CountdownFrame";
import { clearGuided } from "../hooks/useGuidedState";
import { useI18n } from "../i18n";
import { creatorToken, forgetCreatorToken } from "../lib/creator";
import { deleteGift, setGiftPublished } from "../api";

/** Generation phase of a loaded gift (the `ready` pipeline variant). */
type GenPhase =
  /** No audio yet and no job running — shows the Generate button. */
  | { kind: "awaiting" }
  /** A generation job is in flight; `status` is the last SSE progress label. */
  | { kind: "generating"; status: string | null }
  /** Audio is available (gift.audio_url set). */
  | { kind: "ready" }
  /** Terminal failure — the message lives in the shared `error` state. */
  | { kind: "failed" };

/** Countdown poll cadence and total cap: the server retries a failed
 *  generation once, so allow roughly two model calls before giving up. */
const CD_POLL_INTERVAL_MS = 5000;
const CD_POLL_TIMEOUT_MS = 5 * 60 * 1000;

/**
 * Load/generation pipeline for the gift page: loading → ready(gen) | failed.
 * Like/share/delete interaction state deliberately stays out of this union.
 */
type GiftPipeline =
  | { kind: "loading" }
  | { kind: "failed"; message: string }
  | { kind: "ready"; gift: Gift; gen: GenPhase };

export default function GiftPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { t } = useI18n();
  const [pipeline, setPipeline] = useState<GiftPipeline>({ kind: "loading" });
  /** Errors shown inline on a loaded gift: generation + like/publish/delete. */
  const [error, setError] = useState<string | null>(null);
  const [published, setPublished] = useState(false);
  const [publishBusy, setPublishBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [liked, setLiked] = useState(false);
  const [likeCount, setLikeCount] = useState(0);
  const [copied, setCopied] = useState(false);
  const [countdownHtml, setCountdownHtml] = useState<string | null>(null);
  const [countdownPending, setCountdownPending] = useState(false);
  const [countdownFailed, setCountdownFailed] = useState(false);
  const watchRef = useRef<GenerationWatch | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [lrcLines, setLrcLines] = useState<LRCLine[] | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const cdPollRef = useRef<number | undefined>(undefined);

  const startWatch = useCallback(() => {
    watchRef.current?.close();
    if (!id) return;
    watchRef.current = watchGeneration(id, {
      onProgress: (status) =>
        setPipeline((p) =>
          p.kind === "ready" && p.gen.kind === "generating"
            ? { ...p, gen: { ...p.gen, status } }
            : p,
        ),
      onDone: (audioUrl) =>
        setPipeline((p) =>
          p.kind === "ready"
            ? { ...p, gift: { ...p.gift, audio_url: audioUrl, gen_status: "done" }, gen: { kind: "ready" } }
            : p,
        ),
      onFailed: (reason) => {
        setPipeline((p) => (p.kind === "ready" ? { ...p, gen: { kind: "failed" } } : p));
        setError(
          reason === "timeout"
            ? "Generation timed out. Try again."
            : reason === "connection-lost"
              ? "Connection lost during generation"
              : "Music generation failed",
        );
      },
    });
  }, [id]);

  useEffect(() => {
    if (!id) return;
    setPipeline({ kind: "loading" });
    // Reset countdown state left over from a previously viewed gift.
    setCountdownHtml(null);
    setCountdownPending(false);
    setCountdownFailed(false);
    getGift(id)
      .then((g) => {
        setPublished(g.published);
        setLikeCount(g.likes.length);
        const gen: GenPhase = g.audio_url
          ? { kind: "ready" }
          : g.gen_status === "pending" || g.gen_status === "running"
            ? { kind: "generating", status: g.gen_status }
            : { kind: "awaiting" };
        setPipeline({ kind: "ready", gift: g, gen });
        if (gen.kind === "generating") startWatch();
        // Load countdown section if gift has one
        loadCountdown(g.id, g.countdown_status);
        if (g.lrc) setLrcLines(parseLRC(g.lrc));
      })
      .catch((e) =>
        setPipeline({ kind: "failed", message: e instanceof Error ? e.message : "Failed to load gift" }),
      );
    return () => {
      watchRef.current?.close();
      // Stop the countdown poll too — otherwise it keeps firing after
      // unmount, or leaks into the next gift when the id changes.
      clearInterval(cdPollRef.current);
      cdPollRef.current = undefined;
    };
  }, [id, startWatch]);

  async function handleGenerate() {
    if (!id) return;
    // Generate is creator-only (the backend checks X-Creator-Token); a
    // visitor opening a shared link has no token and cannot start a job.
    const token = creatorToken(id);
    if (!token) {
      setError("Only the creator can generate this song");
      return;
    }
    setPipeline((p) => (p.kind === "ready" ? { ...p, gen: { kind: "generating", status: "pending" } } : p));
    setError(null);
    try {
      await generateMusic(id, token);
      startWatch();
    } catch (e) {
      setPipeline((p) => (p.kind === "ready" ? { ...p, gen: { kind: "failed" } } : p));
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

  async function handlePublishToggle(next: boolean) {
    const token = id ? creatorToken(id) : null;
    if (!id || !token) return;
    setPublishBusy(true);
    setError(null);
    const prev = published;
    setPublished(next); // optimistic — revert below if the server disagrees
    try {
      await setGiftPublished(id, token, next);
    } catch (e) {
      setPublished(prev);
      setError(e instanceof Error ? e.message : "Could not change visibility");
    } finally {
      setPublishBusy(false);
    }
  }

  async function handleDelete() {
    const token = id ? creatorToken(id) : null;
    if (!id || !token) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteGift(id, token);
      forgetCreatorToken(id);
      navigate("/playlist");
    } catch (e) {
      setDeleting(false);
      setConfirmDelete(false);
      setError(e instanceof Error ? e.message : "Could not delete this gift");
    }
  }

  function handleShare() {
    const url = window.location.href;
    navigator.clipboard.writeText(url).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  /**
   * Load the LLM-generated countdown block by status: "ready" fetches once;
   * "pending" polls until it turns ready, hits a terminal response, or
   * exceeds the total cap; "failed" shows the placeholder instead of polling
   * a file that will never appear.
   */
  async function loadCountdown(giftId: string, cdStatus?: string | null) {
    if (cdStatus === "failed") {
      setCountdownFailed(true);
      return;
    }
    if (cdStatus === "ready") {
      try {
        const res = await fetch(`/api/countdown-section/${giftId}`);
        if (res.ok) setCountdownHtml(await res.text());
      } catch {
        // Countdown is optional
      }
      return;
    }
    if (cdStatus !== "pending") return;

    setCountdownPending(true);
    const deadline = Date.now() + CD_POLL_TIMEOUT_MS;
    const stopPolling = (failed: boolean) => {
      clearInterval(cdPollRef.current);
      cdPollRef.current = undefined;
      setCountdownPending(false);
      if (failed) setCountdownFailed(true);
    };
    cdPollRef.current = setInterval(async () => {
      if (Date.now() > deadline) {
        stopPolling(true);
        return;
      }
      try {
        const res = await fetch(`/api/countdown-section/${giftId}`);
        if (res.ok) {
          stopPolling(false);
          try {
            setCountdownHtml(await res.text());
          } catch {
            setCountdownFailed(true);
          }
        } else if (res.status === 404) {
          // 404 is terminal (server marked the countdown failed). Pending
          // answers 202, and transient 5xx / network errors keep polling
          // until the cap.
          stopPolling(true);
        }
      } catch {
        // Network hiccup — keep polling until the deadline.
      }
    }, CD_POLL_INTERVAL_MS);
  }

  if (pipeline.kind === "loading") {
    return (
      <div className="gift-page loading-page">
        <span className="spinner" /> {t("loading_gift")}
      </div>
    );
  }

  if (pipeline.kind === "failed") {
    return <div className="gift-page"><div className="error-msg">{pipeline.message}</div></div>;
  }

  const { gift, gen } = pipeline;

  const title = gift.meta.title ?? "Untitled";
  const style = gift.meta.style ?? "";
  const name = gift.meta.name ?? "";
  const relationship = gift.meta.relationship ?? "";
  const degraded = gift.meta.degraded ?? [];
  const showUnwrap = id ? shouldShowUnwrap(id) : false;
  const owned = id ? creatorToken(id) !== null : false;

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
            <span>{t("countdown_creating", { name: name || t("you") })}</span>
          </div>
        )}
        {countdownFailed && (
          <div className="countdown-placeholder">
            <span>{t("countdown_failed")}</span>
          </div>
        )}
        {countdownHtml && <CountdownFrame html={countdownHtml} />}

        {gift.audio_url ? (
          <AudioPlayer src={gift.audio_url} title={title} onTimeUpdate={setCurrentTime} />
        ) : gen.kind === "generating" ? (
          <div className="gift-generating">
            <div className="gen-label">
              {t("gift_creating")}
              <span className="gen-dots">
                <span /><span /><span />
              </span>
            </div>
            <div className="gen-bar-wrap">
              <div className="gen-bar-fill" />
            </div>
            <div className="gen-meta">
              <span>{gen.status || t("gen_preparing")}</span>
              <span>{t("gen_wait")}</span>
            </div>
          </div>
        ) : (
          <button className="btn btn-primary btn-lg btn-full" onClick={() => void handleGenerate()}>
            {t("generate_music")}
          </button>
        )}

        {error && <div className="error-msg">{error}</div>}

        {/* Generation fell back past quality steps (e.g. prompt enrichment) —
            tell the recipient instead of hiding it. */}
        {degraded.length > 0 && <div className="degraded-note">{t("gen_degraded")}</div>}

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
          <Link to="/" className="btn btn-secondary" onClick={clearGuided}>{t("create_another")}</Link>
          <button className="btn btn-secondary" onClick={() => void handleLike()}>
            {liked ? "♥" : "♡"} {likeCount}
          </button>
          <button className="btn btn-secondary" onClick={handleShare}>
            {copied ? t("copied") : t("share")}
          </button>
        </div>

        {owned && (
          <div className="owner-panel">
            <label className="owner-row">
              <span className="owner-label">
                {t("publish_label")}
                <span className="owner-hint">{t("publish_hint")}</span>
              </span>
              <input
                type="checkbox"
                className="owner-switch"
                checked={published}
                disabled={publishBusy}
                onChange={(e) => void handlePublishToggle(e.target.checked)}
              />
            </label>
            {confirmDelete ? (
              <div className="owner-confirm">
                <span className="owner-confirm-q">{t("delete_q")}</span>
                <button className="owner-delete-yes" disabled={deleting} onClick={() => void handleDelete()}>
                  {deleting ? <span className="spinner" /> : t("delete_yes")}
                </button>
                <button className="owner-delete-no" disabled={deleting} onClick={() => setConfirmDelete(false)}>
                  {t("delete_cancel")}
                </button>
              </div>
            ) : (
              <button className="owner-delete" onClick={() => setConfirmDelete(true)}>
                {t("delete_gift")}
              </button>
            )}
          </div>
        )}
      </div>
    </>
  );
}

/** Render lyrics matching reference: parse [Section] markers, <br> between lines. */
function renderLyrics(raw: string) {
  const text = stripMarkers(raw).trim();
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
