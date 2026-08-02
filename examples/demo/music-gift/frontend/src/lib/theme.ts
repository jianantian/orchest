import { useCallback, useEffect, useState } from "react";

/**
 * Theme preference: "auto" follows the OS, "light"/"dark" pin the theme.
 * The painted theme is always carried by `data-theme` on <html> — the dark
 * CSS block in styles.css keys off that attribute, never the media query,
 * so a manual override wins over the OS. The inline boot script in
 * index.html duplicates resolveTheme() to set the attribute before first
 * paint (no FOUC); keep the two in sync.
 */
export type ThemeChoice = "auto" | "light" | "dark";

const KEY = "moment_theme";
const MEDIA = "(prefers-color-scheme: dark)";

export function getThemeChoice(): ThemeChoice {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "auto" || v === "light" || v === "dark") return v;
  } catch {
    // localStorage unavailable
  }
  return "auto";
}

export function resolveTheme(choice: ThemeChoice): "light" | "dark" {
  if (choice !== "auto") return choice;
  return window.matchMedia(MEDIA).matches ? "dark" : "light";
}

function apply(choice: ThemeChoice) {
  document.documentElement.dataset.theme = resolveTheme(choice);
}

export function useTheme(): { choice: ThemeChoice; setChoice: (c: ThemeChoice) => void } {
  const [choice, setChoiceState] = useState<ThemeChoice>(getThemeChoice);

  useEffect(() => {
    apply(choice);
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      // ignore
    }
    if (choice !== "auto") return;
    // Repaint when the OS flips while pinned to "auto".
    const mq = window.matchMedia(MEDIA);
    const onChange = () => apply(choice);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [choice]);

  const setChoice = useCallback((c: ThemeChoice) => setChoiceState(c), []);
  return { choice, setChoice };
}
