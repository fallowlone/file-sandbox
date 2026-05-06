import { test } from "node:test";
import assert from "node:assert/strict";
import Watcher from "./watcher.ts";

class StubJobStore {
  results: unknown[] = [];
  insertReceived() {}
  setStage() {}
  setInQuarantine() {}
  setScanning() {}
  setScanResult(_jobId: string, r: unknown) { this.results.push(r); }
  setRestored() {}
  setPompelmiVerdict() {}
  cancelJob() {}
  fail() {}
}

function makeWatcher(initialMode: "active" | "scan_paused" | "monitoring_disabled") {
  const store = new StubJobStore();
  const w = new Watcher(
    "/tmp/watch-stub",
    [],
    "/tmp/quarantine-stub",
    "test-key",
    store as any,
    {
      initialMode,
      vtEnabled: true,
      pompelmiFailureMode: "bypass",
    },
  );
  return { w, store };
}

test("setMode aborts existing controllers when leaving active", () => {
  const { w } = makeWatcher("active");
  const c1 = new AbortController();
  const c2 = new AbortController();
  // @ts-expect-error: poke private map for the test
  w.scanControllers.set("a", c1);
  // @ts-expect-error
  w.scanControllers.set("b", c2);

  w.setMode("scan_paused");

  assert.equal(c1.signal.aborted, true);
  assert.equal(c2.signal.aborted, true);
});

test("setMode same-state is a no-op", () => {
  const { w } = makeWatcher("scan_paused");
  const c = new AbortController();
  // @ts-expect-error
  w.scanControllers.set("a", c);
  w.setMode("scan_paused");
  assert.equal(c.signal.aborted, false);
});

test("getMode reflects setMode", () => {
  const { w } = makeWatcher("active");
  w.setMode("monitoring_disabled");
  assert.equal(w.getMode(), "monitoring_disabled");
});
