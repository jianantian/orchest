import { useState, useRef, useCallback } from "react";
import type { CreateGiftRequest, GiftMeta } from "../types";
import { createGift, generateMusic } from "../api";

export type MusicGenState = "idle" | "generating" | "ready" | "error";

export interface MusicGenResult {
  giftId: string;
  audioUrl: string | null;
}

export function useMusicGen() {
  const [state, setState] = useState<MusicGenState>("idle");
  const [giftId, setGiftId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const esRef = useRef<EventSource | null>(null);

  /** Open an SSE stream for generation status updates. */
  function watchStream(id: string) {
    // Close any existing stream
    esRef.current?.close();

    const es = new EventSource(`/api/generate/${id}/stream`);
    esRef.current = es;

    es.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.status === "done") {
          es.close();
          setState("ready");
        } else if (data.status === "failed") {
          es.close();
          setState("error");
          setError(data.error ?? "Generation failed");
        }
        // "pending" → keep waiting
      } catch {
        // Ignore malformed events
      }
    };

    es.onerror = () => {
      // EventSource auto-reconnects; if after several attempts it still
      // fails, close and treat as error.
      if (es.readyState === EventSource.CLOSED) {
        setState("error");
        setError("Connection lost during generation");
      }
    };
  }

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

        // Start SSE stream for live status — no polling
        watchStream(id);
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
      watchStream(id);
    } catch (e) {
      setState("error");
      setError(e instanceof Error ? e.message : "Retry failed");
    }
  }, []);

  const reset = useCallback(() => {
    esRef.current?.close();
    esRef.current = null;
    setState("idle");
    setGiftId(null);
    setError(null);
  }, []);

  return { state, giftId, error, start, retry, reset };
}
