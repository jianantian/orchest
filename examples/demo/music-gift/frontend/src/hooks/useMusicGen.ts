import { useState, useRef, useCallback } from "react";
import type { CreateGiftRequest, GiftMeta } from "../types";
import { createGift, generateMusic, getGift, watchGeneration, type GenerationWatch } from "../api";
import { rememberCreatorToken, creatorToken } from "../lib/creator";

export type MusicGenState = "idle" | "generating" | "ready" | "error";

export interface MusicGenResult {
  giftId: string;
  audioUrl: string | null;
}

/**
 * Outcome of one start() call. Returned so callers can branch on failure
 * directly — reading `error` from the hook right after `await start(...)`
 * would see the stale closure from the previous render.
 */
export type MusicGenStartResult =
  | { ok: true; giftId: string }
  | { ok: false; error: string };

export function useMusicGen() {
  const [state, setState] = useState<MusicGenState>("idle");
  const [giftId, setGiftId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const watchRef = useRef<GenerationWatch | null>(null);

  /** Open an SSE watch for generation status updates. */
  function watchStream(id: string) {
    // Close any existing watch
    watchRef.current?.close();
    watchRef.current = watchGeneration(id, {
      onDone: () => setState("ready"),
      onFailed: (reason) => {
        setState("error");
        setError(
          reason === "timeout"
            ? "Generation timed out. Try again."
            : reason === "connection-lost"
              ? "Connection lost during generation"
              : "Generation failed",
        );
      },
    });
  }

  const start = useCallback(
    async (params: {
      lyrics: string;
      style: string;
      title?: string;
      vocal?: string;
      /** "instrumental" is the only kind allowed to have empty lyrics. */
      kind?: "song" | "instrumental";
      meta?: Partial<GiftMeta>;
      lang?: string;
      photos?: string[];
    }): Promise<MusicGenStartResult> => {
      setState("generating");
      setError(null);

      try {
        const req: CreateGiftRequest = {
          lyrics: params.lyrics,
          kind: params.kind ?? "song",
          meta: {
            lang: params.lang ?? "en",
            title: params.title,
            vocal: params.vocal,
            ...params.meta,
          },
          photos: params.photos ?? [],
          style: params.style || "healing and warm",
        };

        const res = await createGift(req);
        const id = res.id;
        // The only time the server hands us this token — keep it or the gift
        // can never be unlisted or deleted from this browser.
        rememberCreatorToken(id, res.creator_token);
        setGiftId(id);

        await generateMusic(id, res.creator_token);

        // Start SSE watch for live status — no polling
        watchStream(id);
        return { ok: true, giftId: id };
      } catch (e) {
        const message = e instanceof Error ? e.message : "Creation failed";
        setState("error");
        setError(message);
        return { ok: false, error: message };
      }
    },
    [],
  );

  const retry = useCallback(async (id: string) => {
    setState("generating");
    setError(null);
    setGiftId(id);
    try {
      // Generate is creator-only: the token from gift creation must still be
      // in this browser, or the retry can never pass the backend check.
      const token = creatorToken(id);
      if (!token) throw new Error("Only the creator can regenerate this song");
      await generateMusic(id, token);
      watchStream(id);
    } catch (e) {
      setState("error");
      setError(e instanceof Error ? e.message : "Retry failed");
    }
  }, []);

  /**
   * Reattach to a persisted gift after a remount (e.g. back from the gift
   * page). Final states need no watch. For "generating" the store is
   * consulted first — a background poller keeps cooking after the client
   * leaves, so the job is often already terminal, and blindly re-opening
   * the SSE watch would re-poll the provider for a finished job.
   */
  const resume = useCallback(async (id: string, state: "generating" | "ready" | "error") => {
    watchRef.current?.close();
    watchRef.current = null;
    setGiftId(id);
    setError(null);
    if (state !== "generating") {
      setState(state);
      return;
    }
    try {
      const gift = await getGift(id);
      if (gift.gen_status === "done") {
        setState("ready");
        return;
      }
      if (gift.gen_status === "failed" || gift.gen_status === "timeout") {
        setState("error");
        setError("Generation failed");
        return;
      }
    } catch {
      // Gift unreadable — fall through to the live watch.
    }
    setState("generating");
    watchStream(id);
  }, []);

  const reset = useCallback(() => {
    watchRef.current?.close();
    watchRef.current = null;
    setState("idle");
    setGiftId(null);
    setError(null);
  }, []);

  return { state, giftId, error, start, retry, resume, reset };
}
