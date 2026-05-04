import { test } from "node:test";
import assert from "node:assert/strict";
import { parseMode, MODES, type WatcherMode } from "./watcher-mode.ts";

test("parseMode returns valid mode unchanged", () => {
  for (const m of MODES) assert.equal(parseMode(m), m);
});

test("parseMode falls back to active for invalid input", () => {
  assert.equal(parseMode("nope"), "active");
  assert.equal(parseMode(undefined), "active");
  assert.equal(parseMode(null), "active");
  assert.equal(parseMode(""), "active");
});

test("MODES contains exactly the three documented modes", () => {
  assert.deepEqual([...MODES].sort(), ["active", "monitoring_disabled", "scan_paused"]);
});
