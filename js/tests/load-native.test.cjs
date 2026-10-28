"use strict";

const assert = require("node:assert/strict");
const { mkdtempSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const test = require("node:test");

const { loadNative } = require("../load-native.js");

function withEmptyRoot(run) {
  const root = mkdtempSync(join(tmpdir(), "orchest-sdk-"));
  try {
    return run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function notFound(id) {
  const error = new Error(`Cannot find module '${id}'`);
  error.code = "MODULE_NOT_FOUND";
  throw error;
}

for (const [platform, arch, musl, label] of [
  ["win32", "x64", false, "win32-x64"],
  ["darwin", "x64", false, "darwin-x64"],
  ["linux", "x64", true, "linux-x64 (musl)"],
]) {
  test(`names ${label} and lists the supported platforms`, () => {
    assert.throws(
      () => loadNative({ platform, arch, musl, requireFn: notFound }),
      {
        message:
          `orchest-sdk does not support ${label}. ` +
          "Supported platforms: linux-x64-gnu, linux-arm64-gnu, darwin-arm64.",
      },
    );
  });
}

test("loads the platform package when no local addon is built", () => {
  withEmptyRoot((packageRoot) => {
    const loaded = [];
    const binding = loadNative({
      platform: "linux",
      arch: "arm64",
      musl: false,
      packageRoot,
      requireFn: (id) => {
        loaded.push(id);
        return { id };
      },
    });
    assert.deepEqual(loaded, ["orchest-sdk-linux-arm64-gnu"]);
    assert.deepEqual(binding, { id: "orchest-sdk-linux-arm64-gnu" });
  });
});

test("prefers a locally built addon", () => {
  withEmptyRoot((packageRoot) => {
    const local = join(packageRoot, "orchest_node.darwin-arm64.node");
    writeFileSync(local, "");
    const loaded = [];
    loadNative({
      platform: "darwin",
      arch: "arm64",
      musl: false,
      packageRoot,
      requireFn: (id) => loaded.push(id),
    });
    assert.deepEqual(loaded, [local]);
  });
});

test("explains a missing platform package", () => {
  withEmptyRoot((packageRoot) => {
    assert.throws(
      () => loadNative({ platform: "linux", arch: "x64", musl: false, packageRoot, requireFn: notFound }),
      (error) => {
        assert.match(error.message, /optional dependency orchest-sdk-linux-x64-gnu is not installed/);
        assert.equal(error.cause.code, "MODULE_NOT_FOUND");
        return true;
      },
    );
  });
});
