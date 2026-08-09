import { useSyncExternalStore } from "react";

/** Reactive matchMedia. The server snapshot is false (desktop-first), same
 *  as the rest of the app's responsive behaviour. */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const mq = window.matchMedia(query);
      mq.addEventListener("change", onChange);
      return () => mq.removeEventListener("change", onChange);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}
