import type { KeyboardEvent } from "react";

/**
 * True while an IME composition (pinyin, Japanese, …) is active. Enter pressed
 * mid-composition only picks a candidate — it must never submit. Gate every
 * Enter keydown handler on this.
 */
export function isImeComposing(e: KeyboardEvent): boolean {
  return e.nativeEvent.isComposing;
}
