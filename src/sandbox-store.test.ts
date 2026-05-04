import { test } from "node:test";
import assert from "node:assert/strict";
import { SandboxStore } from "./sandbox-store.ts";

function fresh() {
  return new SandboxStore(":memory:");
}

test("insert + get round-trip", () => {
  const s = fresh();
  s.insert({
    id: "a",
    vmName: "fsbx-a",
    sourceJobId: null,
    sourceFilePath: "/tmp/f.bin",
    sessionDir: "/tmp/sess/a",
    networkEnabled: false,
  });
  const r = s.get("a");
  assert.equal(r?.id, "a");
  assert.equal(r?.status, "starting");
  assert.equal(r?.networkEnabled, false);
});

test("setRunning sets pid and status", () => {
  const s = fresh();
  s.insert({ id: "a", vmName: "fsbx-a", sourceJobId: null, sourceFilePath: "/x", sessionDir: "/y", networkEnabled: false });
  s.setRunning("a", 4242);
  const r = s.get("a");
  assert.equal(r?.status, "running");
  assert.equal(r?.pid, 4242);
});

test("listSessions returns newest first, capped by limit", () => {
  const s = fresh();
  for (let i = 0; i < 5; i++) {
    s.insert({ id: `a${i}`, vmName: `fsbx-${i}`, sourceJobId: null, sourceFilePath: "/x", sessionDir: "/y", networkEnabled: false });
  }
  const r = s.listSessions({ limit: 3 });
  assert.equal(r.length, 3);
  assert.equal(r[0].id, "a4");
});

test("markDiscarded is idempotent", () => {
  const s = fresh();
  s.insert({ id: "a", vmName: "fsbx-a", sourceJobId: null, sourceFilePath: "/x", sessionDir: "/y", networkEnabled: false });
  s.markDiscarded("a", "test");
  s.markDiscarded("a", "test again");
  const r = s.get("a");
  assert.equal(r?.status, "discarded");
});
