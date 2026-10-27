export { SUPPORTED_PLATFORMS };
export { loadNative };
/**
 * Platforms with a published `orchest-sdk-<key>` package (ADR-0004 D3),
 * keyed by `${process.platform}-${process.arch}`.
 */
declare const SUPPORTED_PLATFORMS: {
    "linux-x64": string;
    "linux-arm64": string;
    "darwin-arm64": string;
};
/**
 * Load the native addon: a locally built `orchest_node.<key>.node` in the
 * package root (`npm run build:native`) wins, otherwise the platform package
 * npm installed as an optional dependency.
 *
 * @param {{ platform?: string, arch?: string, musl?: boolean, packageRoot?: string, requireFn?: (id: string) => unknown }} [options]
 *   Overrides for tests; the defaults describe the running process.
 * @returns {unknown}
 */
declare function loadNative(options?: {
    platform?: string;
    arch?: string;
    musl?: boolean;
    packageRoot?: string;
    requireFn?: (id: string) => unknown;
}): unknown;
