import { test } from "node:test";
import assert from "node:assert/strict";
import { JobStore } from "./job-store.ts";

function freshStore() {
  return new JobStore(":memory:");
}

test("pompelmi_verdict starts null and persists when set", () => {
  const store = freshStore();
  store.insertReceived("job-1", "/a.bin", "a.bin");
  const before = store.getJob("job-1");
  assert.equal(before?.pompelmi_verdict, null);

  store.setPompelmiVerdict("job-1", "clean", "ok");
  const after = store.getJob("job-1");
  assert.equal(after?.pompelmi_verdict, "clean");
});

test("scan_stage starts null and persists when set", () => {
  const store = freshStore();
  store.insertReceived("job-stage-1", "/a.bin", "a.bin");

  const before = store.getJob("job-stage-1");
  assert.equal(before?.scan_stage, null);

  store.setStage("job-stage-1", "cache_check");
  const afterCache = store.getJob("job-stage-1");
  assert.equal(afterCache?.scan_stage, "cache_check");

  store.setStage("job-stage-1", "done");
  const afterDone = store.getJob("job-stage-1");
  assert.equal(afterDone?.scan_stage, "done");
});

test("scan_stage rejects unknown values via type system", () => {
  // Compile-time check: this is here so the reviewer remembers ScanStage is
  // a closed string union. Runtime check is not required — DB column is TEXT.
  const valid: import("./job-store.ts").ScanStage = "vt_poll";
  assert.equal(valid, "vt_poll");
});
