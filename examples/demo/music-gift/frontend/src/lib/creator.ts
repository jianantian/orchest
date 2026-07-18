// Ownership tokens for gifts created on this device.
//
// The backend mints a creator_token per gift and returns it exactly once, in
// the create response — it is never echoed by GET /api/gift/:id, since anyone
// holding a share link can read that. If we lose the token, the gift can no
// longer be unlisted or deleted from this browser.

const KEY = "moment_creator_tokens";

type TokenMap = Record<string, string>;

function load(): TokenMap {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    return parsed && typeof parsed === "object" ? (parsed as TokenMap) : {};
  } catch {
    return {};
  }
}

function save(m: TokenMap) {
  try {
    localStorage.setItem(KEY, JSON.stringify(m));
  } catch {
    // quota or private mode — the gift simply won't be manageable here
  }
}

export function rememberCreatorToken(giftId: string, token: string) {
  const m = load();
  m[giftId] = token;
  save(m);
}

export function creatorToken(giftId: string): string | null {
  return load()[giftId] ?? null;
}

export function forgetCreatorToken(giftId: string) {
  const m = load();
  delete m[giftId];
  save(m);
}

export function isCreator(giftId: string): boolean {
  return creatorToken(giftId) !== null;
}
