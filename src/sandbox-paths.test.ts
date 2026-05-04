import { test } from "node:test";
import assert from "node:assert/strict";
import { validateSandboxSourcePath } from "./sandbox-paths.ts";

test("rejects path traversal", () => {
  assert.throws(
    () =>
      validateSandboxSourcePath("/Users/me/watch/../etc/passwd", {
        watchPath: "/Users/me/watch",
        quarantinePath: "/Users/me/q",
        homeDir: "/Users/me",
      }),
    /outside allowed roots/i,
  );
});

test("accepts a path inside watchPath", () => {
  const out = validateSandboxSourcePath("/Users/me/watch/file.bin", {
    watchPath: "/Users/me/watch",
    quarantinePath: "/Users/me/q",
    homeDir: "/Users/me",
  });
  assert.equal(out, "/Users/me/watch/file.bin");
});

test("rejects relative paths", () => {
  assert.throws(
    () =>
      validateSandboxSourcePath("relative/file", {
        watchPath: "/Users/me/watch",
        quarantinePath: "/Users/me/q",
        homeDir: "/Users/me",
      }),
    /absolute/i,
  );
});
