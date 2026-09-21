/**
 * Package `types` entry.
 *
 * The public surface is the hand-written data types in `types.d.ts` plus the
 * runtime declarations generated from `index.js` into `index.d.ts`. `Agent` is
 * re-exported explicitly from `native.d.ts` so it stays usable as both a value
 * (constructor) and a type.
 */

export * from "./types";
export * from "./index";
export { Agent } from "./native";
