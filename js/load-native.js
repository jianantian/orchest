"use strict";

const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const { join } = require("node:path");

/**
 * Platforms with a published `orchest-sdk-<key>` package (ADR-0004 D3),
 * keyed by `${process.platform}-${process.arch}`.
 */
const SUPPORTED_PLATFORMS = {
  "linux-x64": "linux-x64-gnu",
  "linux-arm64": "linux-arm64-gnu",
  "darwin-arm64": "darwin-arm64",
};

/**
 * glibc reports its runtime version in the process report; musl does not.
 * Without a report, `ldd --version` names musl on musl systems (its loader
 * prints to stderr and exits non-zero).
 *
 * @returns {boolean}
 */
function detectMusl() {
  if (process.platform !== "linux") {
    return false;
  }
  try {
    const report = /** @type {{ header?: { glibcVersionRuntime?: string } }} */ (process.report.getReport());
    return !(report.header && report.header.glibcVersionRuntime);
  } catch {
    // Fall through to ldd.
  }
  const ldd = spawnSync("ldd", ["--version"], { encoding: "utf8" });
  return `${ldd.stdout ?? ""}${ldd.stderr ?? ""}`.toLowerCase().includes("musl");
}

/**
 * @param {string} platform
 * @param {string} arch
 * @param {boolean} musl
 * @returns {string | undefined}
 */
function platformKey(platform, arch, musl) {
  if (musl) {
    return undefined;
  }
  return SUPPORTED_PLATFORMS[/** @type {keyof typeof SUPPORTED_PLATFORMS} */ (`${platform}-${arch}`)];
}

/**
 * Load the native addon: a locally built `orchest_node.<key>.node` in the
 * package root (`npm run build:native`) wins, otherwise the platform package
 * npm installed as an optional dependency.
 *
 * @param {{ platform?: string, arch?: string, musl?: boolean, packageRoot?: string, requireFn?: (id: string) => unknown }} [options]
 *   Overrides for tests; the defaults describe the running process.
 * @returns {unknown}
 */
function loadNative(options = {}) {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const musl = options.musl ?? detectMusl();
  const packageRoot = options.packageRoot ?? join(__dirname, "..");
  const requireFn = options.requireFn ?? require;

  const key = platformKey(platform, arch, musl);
  if (key === undefined) {
    const current = `${platform}-${arch}${musl ? " (musl)" : ""}`;
    const supported = Object.values(SUPPORTED_PLATFORMS).join(", ");
    throw new Error(`orchest-sdk does not support ${current}. Supported platforms: ${supported}.`);
  }

  const local = join(packageRoot, `orchest_node.${key}.node`);
  if (existsSync(local)) {
    return requireFn(local);
  }

  const packageName = `orchest-sdk-${key}`;
  try {
    return requireFn(packageName);
  } catch (error) {
    const code = /** @type {{ code?: unknown }} */ (error).code;
    if (code !== "MODULE_NOT_FOUND") {
      throw error;
    }
    throw new Error(
      `orchest-sdk could not find its native addon for ${key}: the optional dependency ` +
        `${packageName} is not installed. Reinstall orchest-sdk without --omit=optional ` +
        `(or --no-optional), or run \`npm run build:native\` in a source checkout.`,
      { cause: error },
    );
  }
}

exports.SUPPORTED_PLATFORMS = SUPPORTED_PLATFORMS;
exports.loadNative = loadNative;
