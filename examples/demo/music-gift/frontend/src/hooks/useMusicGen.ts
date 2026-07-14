import { useState, useRef, useCallback } from "react";
import type { CreateGiftRequest, GiftMeta } from "../types";
import { createGift, generateMusic, pollGenerateStatus } from "../api";

export type MusicGenState = "idle" | "generating" | "ready" | "error";

export interface MusicGenResult {
  giftId: string;
  audioUrl: string | null;
}

export function useMusicGen() {
  const [state, setState] = useState<MusicGenState>("idle");
  const [giftId, setGiftId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  const start = useCallback(
    async (params: {
      lyrics: string;
      style: string;
      title?: string;
      vocal?: string;
      meta?: Partial<GiftMeta>;
      lang?: string;
      photos?: string[];
    }) => {
      setState("generating");
      setError(null);

      try {
        const req: CreateGiftRequest = {
          lyrics: params.lyrics,
          kind: "song",
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
        setGiftId(id);

        await generateMusic(id);

        clearInterval(pollRef.current);
        pollRef.current = setInterval(async () => {
          try {
            const status = await pollGenerateStatus(id);
            if (status.status === "done") {
              clearInterval(pollRef.current);
              setState("ready");
            } else if (status.status === "failed") {
              clearInterval(pollRef.current);
              setState("error");
              setError("Generation failed");
            }
          } catch {
            // Keep polling on transient errors
          }
        }, 3000);
      } catch (e) {
        setState("error");
        setError(e instanceof Error ? e.message : "Creation failed");
      }
    },
    [],
  );

  const retry = useCallback(async (id: string) => {
    setState("generating");
    setError(null);
    setGiftId(id);
    try {
      await generateMusic(id);
      clearInterval(pollRef.current);
      pollRef.current = setInterval(async () => {
        try {
          const status = await pollGenerateStatus(id);
          if (status.status === "done") {
            clearInterval(pollRef.current);
            setState("ready");
          } else if (status.status === "failed") {
            clearInterval(pollRef.current);
            setState("error");
            setError("Generation failed");
          }
        } catch {
          // Keep polling
        }
      }, 3000);
    } catch (e) {
      setState("error");
      setError(e instanceof Error ? e.message : "Retry failed");
    }
  }, []);

  const reset = useCallback(() => {
    clearInterval(pollRef.current);
    setState("idle");
    setGiftId(null);
    setError(null);
  }, []);

  return { state, giftId, error, start, retry, reset };
}
